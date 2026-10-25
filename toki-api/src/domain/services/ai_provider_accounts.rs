//! A metered payer's accounts: seat refreshes from its account directory,
//! dispositions, and the admin overview with suggestions.

use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use async_trait::async_trait;
use futures::{stream, StreamExt};

use crate::domain::{
    models::{
        account_overview, account_suggestions, directory_refresh_due, review_window, user_activity,
        utc_day, AiAcceptedSuggestions, AiAccountDisposition, AiAccountDispositionId,
        AiAccountDispositionTerms, AiAccountOverviewInput, AiAccountTarget, AiBillingMonth,
        AiDirectoryHealth, AiDirectoryOverview, AiDirectoryRefresh, AiProvider,
        AiProviderAccountId, AiProviderAccountOverview, AiProviderAccountProfile,
        AiSuggestionConfidence, AiSuggestionSkip, AiUsageDateRange, UserId,
        MAX_ACCEPTED_SUGGESTIONS,
    },
    ports::{
        inbound::AiProviderAccountService,
        outbound::{AiProviderAccountRepository, ProviderAccountDirectory},
    },
    AiProviderAccountError, ProviderAccountDirectoryError,
};

/// How many profiles a refresh reads at once.
const PROFILE_CONCURRENCY: usize = 4;
/// The longest refresh error stored, in characters.
const MAX_STORED_ERROR_CHARS: usize = 2000;

/// Whether the payer's account directory can be read.
pub enum AiAccountDirectoryState {
    Ready(Arc<dyn ProviderAccountDirectory>),
    /// No credentials are configured.
    Disabled,
    /// Credentials are configured but unusable, for the given reason.
    Misconfigured(String),
}

pub struct AiProviderAccountServiceImpl<R> {
    repository: Arc<R>,
    directory: AiAccountDirectoryState,
    payer: AiProvider,
    namespace: String,
    refresh_lock: tokio::sync::Mutex<()>,
}

impl<R> AiProviderAccountServiceImpl<R> {
    /// `payer` is whose accounts these are, such as Copilot, and `namespace`
    /// where they live, such as the GitHub organization; both are shown, and
    /// the namespace's words are ignored in logins.
    pub fn new(
        repository: Arc<R>,
        directory: AiAccountDirectoryState,
        payer: AiProvider,
        namespace: String,
    ) -> Self {
        Self {
            repository,
            directory,
            payer,
            namespace,
            refresh_lock: tokio::sync::Mutex::new(()),
        }
    }

    fn ready_directory(
        &self,
    ) -> Result<&Arc<dyn ProviderAccountDirectory>, AiProviderAccountError> {
        match &self.directory {
            AiAccountDirectoryState::Ready(directory) => Ok(directory),
            AiAccountDirectoryState::Disabled => Err(AiProviderAccountError::DirectoryOff),
            AiAccountDirectoryState::Misconfigured(message) => Err(
                AiProviderAccountError::DirectoryMisconfigured(message.clone()),
            ),
        }
    }

    fn health(&self) -> AiDirectoryHealth {
        match &self.directory {
            AiAccountDirectoryState::Ready(_) => AiDirectoryHealth::Ready,
            AiAccountDirectoryState::Disabled => AiDirectoryHealth::Disabled,
            AiAccountDirectoryState::Misconfigured(message) => {
                AiDirectoryHealth::Misconfigured(message.clone())
            }
        }
    }
}

impl<R: AiProviderAccountRepository> AiProviderAccountServiceImpl<R> {
    /// Today's UTC date by the store's clock and the review window ending
    /// with it.
    async fn window(&self) -> Result<AiUsageDateRange, AiProviderAccountError> {
        let today = utc_day(self.repository.now().await?);
        review_window(today).ok_or_else(|| {
            AiProviderAccountError::Storage(format!("no review window ends on {today}"))
        })
    }

    /// Records a failed attempt, best effort: the refresh's own error matters
    /// more.
    async fn record_failure(&self, error: &str) {
        let error: String = error.chars().take(MAX_STORED_ERROR_CHARS).collect();
        if let Err(storage) = self.repository.record_failure(self.payer, &error).await {
            tracing::warn!(error = %storage, "could not record a failed provider account refresh");
        }
    }

    /// One refresh; the caller holds `refresh_lock`.
    async fn refresh_now(
        &self,
        directory: &Arc<dyn ProviderAccountDirectory>,
    ) -> Result<AiDirectoryRefresh, AiProviderAccountError> {
        let started_at = self.repository.now().await?;
        let listing = match directory.seats().await {
            Ok(listing) => listing,
            Err(error) => {
                self.record_failure(&error.to_string()).await;
                return Err(AiProviderAccountError::Directory(error));
            }
        };
        let complete = listing.is_complete();
        let seats_listed = u32::try_from(listing.seats.len()).unwrap_or(u32::MAX);
        let (seats_started, seats_ended) = match self
            .repository
            .store_seat_listing(self.payer, &listing)
            .await
        {
            Ok(counts) => counts,
            Err(error) => {
                self.record_failure(&error.to_string()).await;
                return Err(error);
            }
        };

        // Every stored account, ended seats included, so renames are followed.
        let accounts = self.repository.account_ids(self.payer).await?;
        let profiles = read_profiles(directory.as_ref(), &accounts).await;
        let profiles_read = u32::try_from(profiles.len()).unwrap_or(u32::MAX);
        let profiles_failed = u32::try_from(accounts.len() - profiles.len()).unwrap_or(u32::MAX);
        self.repository
            .store_profiles(self.payer, &profiles)
            .await?;
        self.repository
            .record_success(self.payer, seats_listed, listing.reported_total)
            .await?;

        let refresh = AiDirectoryRefresh {
            started_at,
            finished_at: self.repository.now().await?,
            seats_listed,
            seats_reported: listing.reported_total,
            complete,
            seats_started,
            seats_ended,
            profiles_read,
            profiles_failed,
        };
        tracing::info!(
            payer = self.payer.as_str(),
            seats_listed,
            seats_reported = ?refresh.seats_reported,
            complete,
            seats_started,
            seats_ended,
            profiles_read,
            profiles_failed,
            "refreshed provider accounts"
        );
        Ok(refresh)
    }
}

/// Profiles of `accounts`, read a few at a time. An account without a
/// profile, or whose read fails, is left out, so its stored profile stays. The
/// first rate limit stops further reads.
async fn read_profiles(
    directory: &dyn ProviderAccountDirectory,
    accounts: &[AiProviderAccountId],
) -> Vec<AiProviderAccountProfile> {
    let rate_limited = AtomicBool::new(false);
    let rate_limited = &rate_limited;
    stream::iter(accounts.iter().copied())
        .map(|account| async move {
            if rate_limited.load(Ordering::Relaxed) {
                return None;
            }
            match directory.profile(account).await {
                Ok(profile) => profile,
                Err(error) => {
                    if matches!(error, ProviderAccountDirectoryError::RateLimited { .. }) {
                        rate_limited.store(true, Ordering::Relaxed);
                    }
                    tracing::warn!(%account, %error, "could not read a provider account profile");
                    None
                }
            }
        })
        .buffer_unordered(PROFILE_CONCURRENCY)
        .filter_map(|profile| async move { profile })
        .collect()
        .await
}

#[async_trait]
impl<R: AiProviderAccountRepository> AiProviderAccountService for AiProviderAccountServiceImpl<R> {
    async fn overview(&self) -> Result<AiProviderAccountOverview, AiProviderAccountError> {
        let window = self.window().await?;
        let metered = self.repository.metered_usage(self.payer, window).await?;
        let accounts = self.repository.accounts(self.payer).await?;
        let users = self.repository.users().await?;
        let status = self.repository.directory_status(self.payer).await?;
        // The review window holds the current month's days through today.
        let credits = self
            .repository
            .account_credit_days(self.payer, window)
            .await?;
        let today = window.last_day();
        let credits_month = AiBillingMonth::containing(today)
            .ok_or_else(|| AiProviderAccountError::Storage(format!("no month contains {today}")))?;
        let overview = account_overview(AiAccountOverviewInput {
            today,
            review_from: window.start(),
            accounts,
            users,
            metered,
            namespace: self.namespace.clone(),
            credits,
            credits_month,
        })?;

        Ok(AiProviderAccountOverview {
            payer: self.payer,
            directory: AiDirectoryOverview {
                health: self.health(),
                namespace: self.namespace.clone(),
                status,
            },
            overview,
        })
    }

    async fn refresh(&self) -> Result<AiDirectoryRefresh, AiProviderAccountError> {
        let directory = self.ready_directory()?;
        let _single_flight = self.refresh_lock.lock().await;
        self.refresh_now(directory).await
    }

    async fn refresh_if_due(&self) -> Result<Option<AiDirectoryRefresh>, AiProviderAccountError> {
        let directory = self.ready_directory()?;
        let _single_flight = self.refresh_lock.lock().await;
        let status = self.repository.directory_status(self.payer).await?;
        let now = self.repository.now().await?;
        if !directory_refresh_due(status.as_ref(), now) {
            return Ok(None);
        }
        self.refresh_now(directory).await.map(Some)
    }

    async fn create_disposition(
        &self,
        account: AiProviderAccountId,
        terms: AiAccountDispositionTerms,
        by: UserId,
    ) -> Result<AiAccountDisposition, AiProviderAccountError> {
        self.repository
            .insert_disposition(self.payer, account, &terms, &by)
            .await
    }

    async fn update_disposition(
        &self,
        id: AiAccountDispositionId,
        terms: AiAccountDispositionTerms,
        by: UserId,
    ) -> Result<AiAccountDisposition, AiProviderAccountError> {
        self.repository
            .update_disposition(self.payer, id, &terms, &by)
            .await
    }

    async fn delete_disposition(
        &self,
        id: AiAccountDispositionId,
    ) -> Result<(), AiProviderAccountError> {
        self.repository.delete_disposition(self.payer, id).await
    }

    async fn accept_suggestions(
        &self,
        picks: &[(AiProviderAccountId, UserId)],
        by: UserId,
    ) -> Result<AiAcceptedSuggestions, AiProviderAccountError> {
        if picks.is_empty() || picks.len() > MAX_ACCEPTED_SUGGESTIONS {
            return Err(AiProviderAccountError::Invalid(format!(
                "send 1 to {MAX_ACCEPTED_SUGGESTIONS} picks"
            )));
        }

        // The suggestions are recomputed, so only what the server would
        // suggest now is accepted.
        let window = self.window().await?;
        let today = window.last_day();
        let accounts = self.repository.accounts(self.payer).await?;
        let users = self.repository.users().await?;
        let metered = self.repository.metered_usage(self.payer, window).await?;
        let activity = user_activity(&metered)?;
        let suggestions =
            account_suggestions(&accounts, &users, &activity, &self.namespace, window);
        let accounts: HashMap<_, _> = accounts
            .iter()
            .map(|account| (account.account_id, account))
            .collect();

        let mut accepted = AiAcceptedSuggestions {
            created: Vec::new(),
            skipped: Vec::new(),
        };
        let mut linked = HashSet::new();
        for &(account_id, user_id) in picks {
            let account = accounts.get(&account_id);
            let top = suggestions
                .get(&account_id)
                .and_then(|suggestions| suggestions.first());
            let skip = match (account, top) {
                (Some(account), _) if !account.dispositions.is_empty() => {
                    Some(AiSuggestionSkip::AlreadyDisposed)
                }
                _ if linked.contains(&account_id) => Some(AiSuggestionSkip::AlreadyDisposed),
                (Some(_), Some(top)) if top.user_id == user_id => (top.confidence
                    != AiSuggestionConfidence::Verified)
                    .then_some(AiSuggestionSkip::NotVerified),
                _ => Some(AiSuggestionSkip::Changed),
            };
            let period = account.and_then(|account| account.default_period(today, window));
            let (None, Some(period)) = (skip, period) else {
                accepted.skipped.push((
                    account_id,
                    skip.unwrap_or(AiSuggestionSkip::AlreadyDisposed),
                ));
                continue;
            };

            let terms = AiAccountDispositionTerms {
                target: AiAccountTarget::User(user_id),
                period,
            };
            match self
                .repository
                .insert_disposition(self.payer, account_id, &terms, &by)
                .await
            {
                Ok(disposition) => {
                    linked.insert(account_id);
                    accepted.created.push(disposition);
                }
                Err(AiProviderAccountError::AccountOverlap { .. }) => accepted
                    .skipped
                    .push((account_id, AiSuggestionSkip::AccountOverlap)),
                Err(AiProviderAccountError::UserOverlap { .. }) => accepted
                    .skipped
                    .push((account_id, AiSuggestionSkip::UserOverlap)),
                Err(error) => return Err(error),
            }
        }

        Ok(accepted)
    }
}

#[cfg(test)]
mod tests;

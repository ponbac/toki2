use core::fmt;
use std::{collections::HashMap, sync::Arc, time::Duration};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use az_devops::{Identity, RepoClient};
use futures_util::{stream, StreamExt, TryStreamExt};
use serde::Serialize;
use time::OffsetDateTime;
use tokio::sync::{mpsc, RwLock};
use tracing::{field, instrument, Span};

use crate::domain::Email;

use super::{NotificationHandler, PullRequest, PullRequestDiff, RepoKey};

#[derive(Debug, thiserror::Error)]
pub enum RepoDifferError {
    #[error("Could not fetch pull requests for repo")]
    PullRequests,
    #[error("Could not fetch threads for pull request")]
    Threads,
    #[error("Could not fetch commits for pull request")]
    Commits,
    #[error("Could not fetch work items for pull request")]
    WorkItems,
    #[error("Could not fetch identities")]
    Identities,
}

#[derive(Debug, thiserror::Error)]
enum RepoDifferPollError {
    #[error("{0}")]
    Tick(#[from] RepoDifferError),
    #[error("Tick operation timed out")]
    Timeout,
}

struct RepoDifferTickResult {
    pr_count: usize,
    change_events: Vec<PullRequestDiff>,
}

impl IntoResponse for RepoDifferError {
    fn into_response(self) -> Response {
        let status = StatusCode::INTERNAL_SERVER_ERROR;

        (status, self.to_string()).into_response()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub enum RepoDifferStatus {
    Running,
    Stopped,
    Errored,
}

#[derive(Debug, Clone)]
pub enum RepoDifferMessage {
    Start(Duration),
    ForceUpdate,
    Stop,
}

#[derive(Clone)]
pub struct RepoDiffer {
    pub key: RepoKey,
    az_client: RepoClient,
    notification_handler: Arc<NotificationHandler>,
    pub identities: Arc<RwLock<CachedIdentities>>,
    pub prev_pull_requests: Arc<RwLock<Option<Vec<PullRequest>>>>,
    pub status: Arc<RwLock<RepoDifferStatus>>,
    pub last_updated: Arc<RwLock<Option<OffsetDateTime>>>,
    pub interval: Arc<RwLock<Option<Duration>>>,
}

impl RepoDiffer {
    pub fn new(
        key: RepoKey,
        az_client: RepoClient,
        notification_handler: Arc<NotificationHandler>,
    ) -> Self {
        Self {
            key,
            az_client,
            notification_handler,
            identities: Arc::new(RwLock::new(CachedIdentities::new(Duration::from_secs(
                60 * 60, // Refresh identities every hour
            )))),
            prev_pull_requests: Arc::new(RwLock::new(None)),
            status: Arc::new(RwLock::new(RepoDifferStatus::Stopped)),
            last_updated: Arc::new(RwLock::new(None)),
            interval: Arc::new(RwLock::new(None)),
        }
    }

    async fn is_running(&self) -> bool {
        *self.status.read().await == RepoDifferStatus::Running
    }
}

impl RepoDiffer {
    const MAX_RETRIES: usize = 10;
    const INITIAL_RETRY_DELAY: Duration = Duration::from_secs(30);
    const MAX_RETRY_DELAY: Duration = Duration::from_secs(3600);
    const TICK_TIMEOUT: Duration = Duration::from_secs(120);
    const PR_FETCH_CONCURRENCY: usize = 10;

    pub async fn run(&self, mut receiver: mpsc::Receiver<RepoDifferMessage>) {
        let mut tick_interval: Option<tokio::time::Interval> = None;

        loop {
            tokio::select! {
                Some(message) = receiver.recv() => {
                    match message {
                        RepoDifferMessage::Start(duration) => {
                            tracing::debug!(
                                repo.key = %self.key,
                                interval_seconds = duration.as_secs(),
                                "Starting repo differ"
                            );
                            tick_interval = Some(tokio::time::interval(duration));
                            self.interval.write().await.replace(duration);
                            *self.status.write().await = RepoDifferStatus::Running;
                        }
                        RepoDifferMessage::ForceUpdate => {
                            tracing::debug!(repo.key = %self.key, "Forcing repo differ update");
                            self.force_update().await;
                        }
                        RepoDifferMessage::Stop => {
                            tracing::debug!(repo.key = %self.key, "Stopping repo differ");
                            tick_interval = None;
                            self.interval.write().await.take();
                            *self.status.write().await = RepoDifferStatus::Stopped;
                        }
                    }
                }
                _ = interval_tick_or_sleep(&mut tick_interval) => {
                    tracing::debug!(repo.key = %self.key, "Repo differ interval ticked");
                    self.poll_interval().await;
                }
            }
        }
    }

    async fn force_update(&self) {
        let _ = self.run_poll_attempts("force_update", 1, false).await;
    }

    async fn poll_interval(&self) {
        if let Err(last_error) = self
            .run_poll_attempts("interval", Self::MAX_RETRIES, true)
            .await
        {
            tracing::error!(
                repo.key = %self.key,
                last_error = last_error.as_deref().unwrap_or("unknown"),
                "All repo differ poll retry attempts failed"
            );
            *self.status.write().await = RepoDifferStatus::Errored;
        }
    }

    async fn run_poll_attempts(
        &self,
        trigger: &'static str,
        max_attempts: usize,
        require_running: bool,
    ) -> Result<(), Option<String>> {
        let mut last_error: Option<String> = None;

        for attempt in 0..max_attempts {
            if require_running && !self.is_running().await {
                return Ok(());
            }

            match self.poll_attempt(trigger, attempt).await {
                Ok(()) => return Ok(()),
                Err(err) => {
                    last_error = Some(err.to_string());
                }
            }

            let next_attempt = attempt + 1;
            if next_attempt < max_attempts {
                let backoff_duration = Self::calculate_backoff_duration(next_attempt);
                tracing::warn!(
                    repo.key = %self.key,
                    retry_attempt = next_attempt,
                    max_retries = max_attempts,
                    backoff_seconds = backoff_duration.as_secs_f64(),
                    "Retrying repo differ poll after failed attempt"
                );
                tokio::time::sleep(backoff_duration).await;
            }
        }

        Err(last_error)
    }

    #[instrument(
        name = "repo_differ.poll",
        skip(self),
        fields(
            repo.key = %self.key,
            operation.name = "repo_differ.poll",
            trigger = trigger,
            retry_attempt = retry_attempt,
            timeout_seconds = 120_u64,
            pr_count = field::Empty,
            changed_pr_count = field::Empty,
            notification_count = field::Empty,
            push_notification_count = field::Empty,
            notification_error = field::Empty,
            otel.status_code = field::Empty,
            otel.status_description = field::Empty,
            error.message = field::Empty,
        )
    )]
    async fn poll_attempt(
        &self,
        trigger: &'static str,
        retry_attempt: usize,
    ) -> Result<(), RepoDifferPollError> {
        let tick_result = match tokio::time::timeout(Self::TICK_TIMEOUT, self.tick()).await {
            Ok(Ok(result)) => result,
            Ok(Err(err)) => {
                mark_current_span_error(&err.to_string());
                tracing::error!(error.message = %err, "Repo differ tick failed");
                return Err(RepoDifferPollError::Tick(err));
            }
            Err(_) => {
                let err = RepoDifferPollError::Timeout;
                mark_current_span_error(&err.to_string());
                tracing::error!(
                    timeout_seconds = Self::TICK_TIMEOUT.as_secs(),
                    "Repo differ tick timed out"
                );
                return Err(err);
            }
        };

        let pr_count = tick_result.pr_count;
        let changed_pr_count = tick_result.change_events.len();
        Span::current().record("pr_count", pr_count);
        Span::current().record("changed_pr_count", changed_pr_count);

        let (notification_count, push_notification_count, notification_error) =
            if tick_result.change_events.is_empty() {
                tracing::debug!("No changes to notify");
                (0, 0, false)
            } else {
                match self
                    .notification_handler
                    .notify_affected_users(tick_result.change_events)
                    .await
                {
                    Ok(summary) => (
                        summary.notification_count,
                        summary.push_notification_count,
                        false,
                    ),
                    Err(err) => {
                        let message = format!("Failed to notify affected users: {err}");
                        mark_current_span_error(&message);
                        tracing::error!(error.message = %err, "Failed to notify affected users");
                        (0, 0, true)
                    }
                }
            };

        Span::current().record("notification_count", notification_count);
        Span::current().record("push_notification_count", push_notification_count);
        Span::current().record("notification_error", notification_error);

        tracing::info!(
            pr_count,
            changed_pr_count,
            notification_count,
            push_notification_count,
            notification_error,
            "Repo differ poll completed"
        );

        Ok(())
    }

    fn calculate_backoff_duration(retry_count: usize) -> Duration {
        let base = Self::INITIAL_RETRY_DELAY.as_secs_f64();
        let max = Self::MAX_RETRY_DELAY.as_secs_f64();

        // initial_delay * 2^retry_count
        let exp_backoff = base * (2_f64.powi(retry_count as i32));
        let final_delay = exp_backoff.min(max);

        Duration::from_secs_f64(final_delay)
    }

    #[instrument(name = "repo_differ.fetch_and_diff", skip(self), fields(repo.key = %self.key, operation.name = "repo_differ.fetch_and_diff", pr_count = field::Empty, changed_pr_count = field::Empty))]
    async fn tick(&self) -> Result<RepoDifferTickResult, RepoDifferError> {
        let base_pull_requests = self
            .az_client
            .get_open_pull_requests()
            .await
            .map_err(|_| RepoDifferError::PullRequests)?;
        tracing::Span::current().record("pr_count", base_pull_requests.len());

        let pr_ids = base_pull_requests
            .iter()
            .map(|pr| pr.id)
            .collect::<Vec<_>>();
        let mut work_items_by_pr = self
            .az_client
            .get_work_items_for_pull_requests(&pr_ids)
            .await
            .map_err(|_| RepoDifferError::WorkItems)?;

        let mut fetched_pull_requests = stream::iter(base_pull_requests.into_iter().enumerate())
            .map(|(index, pr)| {
                let client = self.az_client.clone();
                async move {
                    let (commits, threads) = tokio::try_join!(
                        async {
                            pr.commits(&client)
                                .await
                                .map_err(|_| RepoDifferError::Commits)
                        },
                        async {
                            pr.threads(&client)
                                .await
                                .map_err(|_| RepoDifferError::Threads)
                        },
                    )?;

                    Ok::<_, RepoDifferError>((index, pr, commits, threads))
                }
            })
            .buffer_unordered(Self::PR_FETCH_CONCURRENCY)
            .try_collect::<Vec<_>>()
            .await?;
        fetched_pull_requests.sort_by_key(|(index, _, _, _)| *index);

        let mut complete_pull_requests = Vec::with_capacity(fetched_pull_requests.len());
        for (_, pr, commits, threads) in fetched_pull_requests {
            let work_items = work_items_by_pr.remove(&pr.id).unwrap_or_default();

            let url = format!(
                "https://dev.azure.com/{}/{}/_git/{}/pullrequest/{}",
                self.key.organization, self.key.project, self.key.repo_name, pr.id
            );
            complete_pull_requests.push(PullRequest::new(
                &self.key, url, pr, threads, commits, work_items,
            ));
        }

        let id_to_email_map = {
            let cached_identities = self.identities.read().await;
            // Update the cached identities if they are stale.
            if cached_identities.is_stale() {
                let identities = self
                    .az_client
                    .get_git_identities()
                    .await
                    .map_err(|_| RepoDifferError::Identities)?;

                drop(cached_identities); // Drop the read lock before acquiring write lock to avoid deadlock
                let mut cached_identities = self.identities.write().await;
                cached_identities.update(identities);
                cached_identities.id_to_email_map()
            } else {
                cached_identities.id_to_email_map()
            }
        };

        let change_events = {
            let prev_pull_requests = self.prev_pull_requests.read().await;
            match prev_pull_requests.clone() {
                Some(prev_pull_requests) => prev_pull_requests
                    .iter()
                    .map(|prev_pr| {
                        prev_pr.changelog(
                            complete_pull_requests
                                .iter()
                                .find(|p| p.pull_request_base.id == prev_pr.pull_request_base.id),
                            &id_to_email_map,
                        )
                    })
                    .filter(|diff| !diff.changes.is_empty())
                    .collect::<Vec<PullRequestDiff>>(),
                None => Vec::new(),
            }
        };
        let pr_count = complete_pull_requests.len();
        tracing::Span::current().record("changed_pr_count", change_events.len());
        tracing::info!(
            pr_count,
            changed_pr_count = change_events.len(),
            "Repo differ calculated pull request changes"
        );

        self.prev_pull_requests
            .write()
            .await
            .replace(complete_pull_requests);
        self.last_updated
            .write()
            .await
            .replace(OffsetDateTime::now_utc());

        Ok(RepoDifferTickResult {
            pr_count,
            change_events,
        })
    }
}

fn mark_current_span_error(message: &str) {
    let span = Span::current();
    span.record("otel.status_code", "ERROR");
    span.record("otel.status_description", message);
    span.record("error.message", message);
}

impl fmt::Debug for RepoDiffer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RepoDiffer")
            .field("key", &self.key)
            .field("prev_pull_requests", &self.prev_pull_requests)
            .field("last_updated", &self.last_updated)
            .finish()
    }
}

async fn interval_tick_or_sleep(interval: &mut Option<tokio::time::Interval>) {
    if let Some(interval) = interval {
        interval.tick().await;
    } else {
        // Sleep for a very long time to mimic a pending future.
        tokio::time::sleep(tokio::time::Duration::from_secs(86400)).await;
    }
}

#[derive(Debug, Clone, Default)]
pub struct CachedIdentities {
    pub identities: Vec<Identity>,
    last_updated: Option<OffsetDateTime>,
    stale_after: Duration,
}

impl CachedIdentities {
    pub fn new(stale_after: Duration) -> Self {
        Self {
            identities: Vec::new(),
            last_updated: None,
            stale_after,
        }
    }

    pub fn is_stale(&self) -> bool {
        self.last_updated.is_none_or(|last_updated| {
            (OffsetDateTime::now_utc() - last_updated).unsigned_abs() > self.stale_after
        })
    }

    /// Update the cached identities and set the last updated time to now.
    pub fn update(&mut self, identities: Vec<Identity>) {
        self.identities = identities;
        self.last_updated = Some(OffsetDateTime::now_utc());
    }

    pub fn id_to_name_map(&self) -> HashMap<String, String> {
        self.identities
            .iter()
            .map(|i| (i.id.to_uppercase(), i.display_name.clone()))
            .collect::<HashMap<_, _>>()
    }

    pub fn id_to_email_map(&self) -> HashMap<String, Email> {
        self.identities
            .iter()
            .filter_map(|i| {
                Email::try_from(i.unique_name.as_str())
                    .map(|email| (i.id.to_uppercase(), email))
                    .ok()
            })
            .collect::<HashMap<_, _>>()
    }
}

use std::{
    sync::{atomic::AtomicUsize, Mutex},
    time::Duration,
};

use time::{
    macros::{date, datetime},
    OffsetDateTime,
};

use super::*;
use crate::domain::models::{
    AiAccountDispositionTerms, AiBillingDeveloper, AiDirectoryStatus, AiListedSeat,
    AiMeteredUserDay, AiProviderLogin, AiSeatListing, AiStoredProviderAccount,
};

const NOW: OffsetDateTime = datetime!(2026-10-24 10:00 UTC);

fn account(id: i64) -> AiProviderAccountId {
    AiProviderAccountId::new(id).unwrap()
}

fn listing(ids: impl IntoIterator<Item = i64>) -> AiSeatListing {
    let seats: Vec<AiListedSeat> = ids
        .into_iter()
        .map(|id| AiListedSeat {
            account_id: account(id),
            login: AiProviderLogin::parse(&format!("user-{id}")).unwrap(),
            created_at: datetime!(2026-01-01 08:00 UTC),
            pending_cancellation_on: None,
            last_activity_at: None,
            last_activity_client: None,
        })
        .collect();
    AiSeatListing {
        reported_total: Some(u32::try_from(seats.len()).unwrap()),
        seats,
        ended_on_short_page: true,
    }
}

/// A directory with a fixed listing that counts its calls.
struct FakeDirectory {
    listing: AiSeatListing,
    seats_error: Option<ProviderAccountDirectoryError>,
    seats_delay: Duration,
    seats_calls: AtomicUsize,
    in_flight: AtomicUsize,
    most_in_flight: AtomicUsize,
    profile_calls: AtomicUsize,
    /// The call, counting from 1, that GitHub answers with a rate limit.
    rate_limited_call: Option<usize>,
}

impl FakeDirectory {
    fn new(listing: AiSeatListing) -> Self {
        Self {
            listing,
            seats_error: None,
            seats_delay: Duration::ZERO,
            seats_calls: AtomicUsize::new(0),
            in_flight: AtomicUsize::new(0),
            most_in_flight: AtomicUsize::new(0),
            profile_calls: AtomicUsize::new(0),
            rate_limited_call: None,
        }
    }
}

#[async_trait]
impl ProviderAccountDirectory for FakeDirectory {
    async fn seats(&self) -> Result<AiSeatListing, ProviderAccountDirectoryError> {
        self.seats_calls.fetch_add(1, Ordering::SeqCst);
        let in_flight = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
        self.most_in_flight.fetch_max(in_flight, Ordering::SeqCst);
        tokio::time::sleep(self.seats_delay).await;
        self.in_flight.fetch_sub(1, Ordering::SeqCst);
        match &self.seats_error {
            Some(error) => Err(error.clone()),
            None => Ok(self.listing.clone()),
        }
    }

    async fn profile(
        &self,
        account: AiProviderAccountId,
    ) -> Result<Option<AiProviderAccountProfile>, ProviderAccountDirectoryError> {
        let call = self.profile_calls.fetch_add(1, Ordering::SeqCst) + 1;
        if self.rate_limited_call == Some(call) {
            return Err(ProviderAccountDirectoryError::RateLimited { retry_after: None });
        }
        Ok(Some(AiProviderAccountProfile {
            account_id: account,
            login: AiProviderLogin::parse(&format!("user-{account}")).unwrap(),
            name: Some(format!("User {account}")),
            email: None,
        }))
    }
}

/// What the fake repository holds.
#[derive(Default)]
struct Stored {
    accounts: Vec<AiProviderAccountId>,
    listings: usize,
    profiles: Vec<AiProviderAccountProfile>,
    status: Option<AiDirectoryStatus>,
    failures: Vec<String>,
}

/// An in-memory repository for refreshes; disposition and overview reads are
/// covered with Postgres elsewhere.
#[derive(Default)]
struct FakeRepository {
    stored: Mutex<Stored>,
}

#[async_trait]
impl AiProviderAccountRepository for FakeRepository {
    async fn now(&self) -> Result<OffsetDateTime, AiProviderAccountError> {
        Ok(NOW)
    }

    async fn store_seat_listing(
        &self,
        _: AiProvider,
        listing: &AiSeatListing,
    ) -> Result<(u32, u32), AiProviderAccountError> {
        let mut stored = self.stored.lock().unwrap();
        stored.listings += 1;
        let mut started = 0;
        for seat in &listing.seats {
            if !stored.accounts.contains(&seat.account_id) {
                stored.accounts.push(seat.account_id);
                started += 1;
            }
        }
        Ok((started, 0))
    }

    async fn account_ids(
        &self,
        _: AiProvider,
    ) -> Result<Vec<AiProviderAccountId>, AiProviderAccountError> {
        Ok(self.stored.lock().unwrap().accounts.clone())
    }

    async fn store_profiles(
        &self,
        _: AiProvider,
        profiles: &[AiProviderAccountProfile],
    ) -> Result<(), AiProviderAccountError> {
        self.stored
            .lock()
            .unwrap()
            .profiles
            .extend(profiles.iter().cloned());
        Ok(())
    }

    async fn directory_status(
        &self,
        _: AiProvider,
    ) -> Result<Option<AiDirectoryStatus>, AiProviderAccountError> {
        Ok(self.stored.lock().unwrap().status.clone())
    }

    async fn record_success(
        &self,
        _: AiProvider,
        listed: u32,
        reported: Option<u32>,
    ) -> Result<(), AiProviderAccountError> {
        self.stored.lock().unwrap().status = Some(AiDirectoryStatus {
            last_attempt_at: NOW,
            last_success_at: Some(NOW),
            last_error: None,
            seats_listed: Some(listed),
            seats_reported: reported,
        });
        Ok(())
    }

    async fn record_failure(
        &self,
        _: AiProvider,
        error: &str,
    ) -> Result<(), AiProviderAccountError> {
        let mut stored = self.stored.lock().unwrap();
        stored.failures.push(error.to_string());
        let last_success_at = stored
            .status
            .as_ref()
            .and_then(|status| status.last_success_at);
        stored.status = Some(AiDirectoryStatus {
            last_attempt_at: NOW,
            last_success_at,
            last_error: Some(error.to_string()),
            seats_listed: None,
            seats_reported: None,
        });
        Ok(())
    }

    async fn accounts(
        &self,
        _: AiProvider,
    ) -> Result<Vec<AiStoredProviderAccount>, AiProviderAccountError> {
        Ok(Vec::new())
    }

    async fn insert_disposition(
        &self,
        _: AiProvider,
        _: AiProviderAccountId,
        _: &AiAccountDispositionTerms,
        _: &UserId,
    ) -> Result<AiAccountDisposition, AiProviderAccountError> {
        Err(AiProviderAccountError::AccountNotFound)
    }

    async fn update_disposition(
        &self,
        _: AiProvider,
        _: AiAccountDispositionId,
        _: &AiAccountDispositionTerms,
        _: &UserId,
    ) -> Result<AiAccountDisposition, AiProviderAccountError> {
        Err(AiProviderAccountError::NotFound)
    }

    async fn delete_disposition(
        &self,
        _: AiProvider,
        _: AiAccountDispositionId,
    ) -> Result<(), AiProviderAccountError> {
        Err(AiProviderAccountError::NotFound)
    }

    async fn users(&self) -> Result<Vec<AiBillingDeveloper>, AiProviderAccountError> {
        Ok(Vec::new())
    }

    async fn metered_usage(
        &self,
        _: AiProvider,
        _: AiUsageDateRange,
    ) -> Result<Vec<AiMeteredUserDay>, AiProviderAccountError> {
        Ok(Vec::new())
    }
}

fn service(
    directory: Arc<FakeDirectory>,
) -> (
    AiProviderAccountServiceImpl<FakeRepository>,
    Arc<FakeRepository>,
) {
    let repository = Arc::new(FakeRepository::default());
    let service = AiProviderAccountServiceImpl::new(
        repository.clone(),
        AiAccountDirectoryState::Ready(directory),
        AiProvider::Copilot,
        "Example-AB".to_string(),
    );
    (service, repository)
}

#[tokio::test]
async fn a_refresh_stores_seats_and_profiles_and_records_its_success() {
    let directory = Arc::new(FakeDirectory::new(listing([1001, 1002, 1003])));
    let (service, repository) = service(directory.clone());

    let refresh = service.refresh().await.unwrap();
    assert_eq!(
        refresh,
        AiDirectoryRefresh {
            started_at: NOW,
            finished_at: NOW,
            seats_listed: 3,
            seats_reported: Some(3),
            complete: true,
            seats_started: 3,
            seats_ended: 0,
            profiles_read: 3,
            profiles_failed: 0,
        }
    );
    let stored = repository.stored.lock().unwrap();
    assert_eq!(stored.profiles.len(), 3);
    assert_eq!(stored.status.as_ref().unwrap().seats_listed, Some(3));
}

#[tokio::test]
async fn one_refresh_runs_at_a_time() {
    let mut directory = FakeDirectory::new(listing([1001]));
    directory.seats_delay = Duration::from_millis(50);
    let directory = Arc::new(directory);
    let (service, _) = service(directory.clone());

    let (first, second) = tokio::join!(service.refresh(), service.refresh());
    first.unwrap();
    second.unwrap();
    assert_eq!(directory.seats_calls.load(Ordering::SeqCst), 2);
    assert_eq!(directory.most_in_flight.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_scheduled_refresh_runs_only_when_due() {
    let directory = Arc::new(FakeDirectory::new(listing([1001])));
    let (service, repository) = service(directory.clone());

    // Never refreshed: due.
    assert!(service.refresh_if_due().await.unwrap().is_some());
    // Succeeded today: not due, and GitHub is not asked.
    assert!(service.refresh_if_due().await.unwrap().is_none());
    assert_eq!(directory.seats_calls.load(Ordering::SeqCst), 1);

    // Succeeded yesterday: due again.
    repository.stored.lock().unwrap().status = Some(AiDirectoryStatus {
        last_attempt_at: datetime!(2026-10-23 10:00 UTC),
        last_success_at: Some(datetime!(2026-10-23 10:00 UTC)),
        last_error: None,
        seats_listed: Some(1),
        seats_reported: Some(1),
    });
    assert!(service.refresh_if_due().await.unwrap().is_some());
}

#[tokio::test]
async fn a_profile_rate_limit_stops_profile_reads_but_not_the_refresh() {
    let mut directory = FakeDirectory::new(listing(1001..=1010));
    directory.rate_limited_call = Some(3);
    let directory = Arc::new(directory);
    let (service, repository) = service(directory.clone());

    let refresh = service.refresh().await.unwrap();
    let calls = directory.profile_calls.load(Ordering::SeqCst);
    // Reads already under way finish; none starts after the limit.
    assert!(calls <= PROFILE_CONCURRENCY, "{calls} profile calls");
    assert_eq!(refresh.profiles_read as usize, calls - 1);
    assert_eq!(refresh.profiles_read + refresh.profiles_failed, 10);
    assert_eq!(refresh.seats_started, 10);
    let stored = repository.stored.lock().unwrap();
    assert_eq!(stored.listings, 1);
    assert!(stored.failures.is_empty());
    assert_eq!(stored.status.as_ref().unwrap().last_success_at, Some(NOW));
}

#[tokio::test]
async fn a_failed_listing_is_recorded_and_changes_nothing() {
    let mut directory = FakeDirectory::new(listing([1001]));
    directory.seats_error = Some(ProviderAccountDirectoryError::Unavailable(
        "synthetic outage".to_string(),
    ));
    let directory = Arc::new(directory);
    let (service, repository) = service(directory.clone());

    let error = service.refresh().await.unwrap_err();
    assert!(matches!(
        error,
        AiProviderAccountError::Directory(ProviderAccountDirectoryError::Unavailable(_))
    ));
    let stored = repository.stored.lock().unwrap();
    assert_eq!(stored.listings, 0);
    assert_eq!(
        stored.failures,
        ["the account directory is unavailable: synthetic outage"]
    );
    assert_eq!(directory.profile_calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn without_a_usable_directory_a_refresh_is_refused() {
    let off = AiProviderAccountServiceImpl::new(
        Arc::new(FakeRepository::default()),
        AiAccountDirectoryState::Disabled,
        AiProvider::Copilot,
        "Example-AB".to_string(),
    );
    assert!(matches!(
        off.refresh().await,
        Err(AiProviderAccountError::DirectoryOff)
    ));
    assert!(matches!(
        off.refresh_if_due().await,
        Err(AiProviderAccountError::DirectoryOff)
    ));

    let misconfigured = AiProviderAccountServiceImpl::new(
        Arc::new(FakeRepository::default()),
        AiAccountDirectoryState::Misconfigured("github.app_private_key is broken".to_string()),
        AiProvider::Copilot,
        "Example-AB".to_string(),
    );
    assert!(matches!(
        misconfigured.refresh().await,
        Err(AiProviderAccountError::DirectoryMisconfigured(message)) if message.contains("broken")
    ));
    // The overview still works, and says why.
    let overview = misconfigured.overview().await.unwrap();
    assert_eq!(
        overview.directory.health,
        AiDirectoryHealth::Misconfigured("github.app_private_key is broken".to_string())
    );
    assert_eq!(overview.overview.today, date!(2026 - 10 - 24));
    assert_eq!(overview.overview.review_from, date!(2026 - 09 - 01));
}

#[tokio::test]
async fn accepting_suggestions_takes_1_to_100_picks() {
    let (service, _) = service(Arc::new(FakeDirectory::new(listing([]))));
    for count in [0, MAX_ACCEPTED_SUGGESTIONS + 1] {
        let picks = vec![(account(1001), UserId::new(1)); count];
        assert!(matches!(
            service.accept_suggestions(&picks, UserId::new(1)).await,
            Err(AiProviderAccountError::Invalid(_))
        ));
    }
}

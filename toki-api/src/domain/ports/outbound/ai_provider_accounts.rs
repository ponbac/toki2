use async_trait::async_trait;
use time::OffsetDateTime;

use crate::domain::{
    models::{
        AiAccountDisposition, AiAccountDispositionId, AiAccountDispositionTerms,
        AiBillingDeveloper, AiDirectoryStatus, AiMeteredUserDay, AiProvider, AiProviderAccountId,
        AiProviderAccountProfile, AiSeatListing, AiStoredProviderAccount, AiUsageDateRange, UserId,
    },
    AiProviderAccountError, ProviderAccountDirectoryError,
};

/// The accounts a metered payer bills per seat, as its provider lists them.
/// For Copilot: the GitHub accounts holding Copilot seats in the organization.
#[async_trait]
pub trait ProviderAccountDirectory: Send + Sync + 'static {
    /// Every seat listed now, read page by page.
    async fn seats(&self) -> Result<AiSeatListing, ProviderAccountDirectoryError>;

    /// An account's public profile; `None` when the provider has no such
    /// account.
    async fn profile(
        &self,
        account: AiProviderAccountId,
    ) -> Result<Option<AiProviderAccountProfile>, ProviderAccountDirectoryError>;
}

/// Stored provider accounts, their seat history, their dispositions, and the
/// directory's refresh status. Nothing here ever deletes an account or a seat.
#[async_trait]
pub trait AiProviderAccountRepository: Send + Sync + 'static {
    /// The store's clock.
    async fn now(&self) -> Result<OffsetDateTime, AiProviderAccountError>;

    /// Stores a listing in one transaction: new accounts are added and renamed
    /// ones follow, an account listed with another assignment instant ends its
    /// open seat, listed seats are inserted, refreshed or reopened, and, only
    /// when `listing.is_complete()`, open seats of unlisted accounts end.
    /// Returns how many seats started and how many ended.
    async fn store_seat_listing(
        &self,
        payer: AiProvider,
        listing: &AiSeatListing,
    ) -> Result<(u32, u32), AiProviderAccountError>;

    /// Every stored account of `payer`, ended seats included.
    async fn account_ids(
        &self,
        payer: AiProvider,
    ) -> Result<Vec<AiProviderAccountId>, AiProviderAccountError>;

    /// Stores profiles read now, with their logins. Unknown accounts are
    /// ignored.
    async fn store_profiles(
        &self,
        payer: AiProvider,
        profiles: &[AiProviderAccountProfile],
    ) -> Result<(), AiProviderAccountError>;

    async fn directory_status(
        &self,
        payer: AiProvider,
    ) -> Result<Option<AiDirectoryStatus>, AiProviderAccountError>;

    /// Records a successful refresh now, with what the listing held and what
    /// the payer reported.
    async fn record_success(
        &self,
        payer: AiProvider,
        listed: u32,
        reported: Option<u32>,
    ) -> Result<(), AiProviderAccountError>;

    /// Records a failed refresh attempt now. The last success is kept.
    async fn record_failure(
        &self,
        payer: AiProvider,
        error: &str,
    ) -> Result<(), AiProviderAccountError>;

    /// Every stored account of `payer` with its profile, seats and
    /// dispositions, by login.
    async fn accounts(
        &self,
        payer: AiProvider,
    ) -> Result<Vec<AiStoredProviderAccount>, AiProviderAccountError>;

    /// Stores a disposition of a stored account, with the account's current
    /// login. Fails with `AccountOverlap` or `UserOverlap` when the dates share
    /// a day with another disposition of the account or of the user,
    /// `AccountNotFound` and `UserNotFound`.
    async fn insert_disposition(
        &self,
        payer: AiProvider,
        account: AiProviderAccountId,
        terms: &AiAccountDispositionTerms,
        by: &UserId,
    ) -> Result<AiAccountDisposition, AiProviderAccountError>;

    /// Replaces a disposition's target and dates, and its login with the
    /// account's current one. `NotFound` when there is none; overlaps as for
    /// `insert_disposition`.
    async fn update_disposition(
        &self,
        payer: AiProvider,
        id: AiAccountDispositionId,
        terms: &AiAccountDispositionTerms,
        by: &UserId,
    ) -> Result<AiAccountDisposition, AiProviderAccountError>;

    /// Deletes a disposition; `NotFound` when there is none.
    async fn delete_disposition(
        &self,
        payer: AiProvider,
        id: AiAccountDispositionId,
    ) -> Result<(), AiProviderAccountError>;

    /// Every Toki user.
    async fn users(&self) -> Result<Vec<AiBillingDeveloper>, AiProviderAccountError>;

    /// Usage `payer` paid for (`COALESCE(billing_provider, provider)`), per
    /// user and UTC day within `dates`, personal keys included.
    async fn metered_usage(
        &self,
        payer: AiProvider,
        dates: AiUsageDateRange,
    ) -> Result<Vec<AiMeteredUserDay>, AiProviderAccountError>;
}

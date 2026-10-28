use async_trait::async_trait;

use crate::domain::{
    models::{
        AiAcceptedSuggestions, AiAccountDisposition, AiAccountDispositionId,
        AiAccountDispositionTerms, AiDirectoryRefresh, AiProviderAccountId,
        AiProviderAccountOverview, UserId,
    },
    AiProviderAccountError,
};

/// A metered payer's accounts and whose usage each one is. Callers hold admin
/// power.
#[async_trait]
pub trait AiProviderAccountService: Send + Sync + 'static {
    /// Every account with its seats, dispositions and suggestions, and what
    /// needs a look.
    async fn overview(&self) -> Result<AiProviderAccountOverview, AiProviderAccountError>;

    /// Reads seats and profiles from the payer's directory now. One refresh
    /// runs at a time. `DirectoryOff` or `DirectoryMisconfigured` without a
    /// usable directory, and `Directory` when listing seats fails.
    async fn refresh(&self) -> Result<AiDirectoryRefresh, AiProviderAccountError>;

    /// For the scheduler: refreshes only when a refresh is due
    /// (`directory_refresh_due`).
    async fn refresh_if_due(&self) -> Result<Option<AiDirectoryRefresh>, AiProviderAccountError>;

    /// Gives a stored account a disposition.
    async fn create_disposition(
        &self,
        account: AiProviderAccountId,
        terms: AiAccountDispositionTerms,
        by: UserId,
    ) -> Result<AiAccountDisposition, AiProviderAccountError>;

    /// Replaces a disposition's target and dates.
    async fn update_disposition(
        &self,
        id: AiAccountDispositionId,
        terms: AiAccountDispositionTerms,
        by: UserId,
    ) -> Result<AiAccountDisposition, AiProviderAccountError>;

    /// Deletes a disposition as if it never existed.
    async fn delete_disposition(
        &self,
        id: AiAccountDispositionId,
    ) -> Result<(), AiProviderAccountError>;

    /// Links each picked account to its user from the account's default
    /// period, but only while the account has no disposition and the user is
    /// its verified top suggestion. Picks are applied in order; the others are
    /// skipped with a reason. 1 to `MAX_ACCEPTED_SUGGESTIONS` picks.
    async fn accept_suggestions(
        &self,
        picks: &[(AiProviderAccountId, UserId)],
        by: UserId,
    ) -> Result<AiAcceptedSuggestions, AiProviderAccountError>;
}

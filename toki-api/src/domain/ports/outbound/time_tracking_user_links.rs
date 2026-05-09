use async_trait::async_trait;

use crate::domain::{
    models::{
        NewTimeTrackingProviderUser, NewTimeTrackingUserLink, TimeTrackingProviderUser,
        TimeTrackingUserLink, UserId,
    },
    TimeTrackingError,
};

#[async_trait]
pub trait TimeTrackingUserLinkRepository: Send + Sync + 'static {
    async fn upsert_provider_users(
        &self,
        users: &[NewTimeTrackingProviderUser],
    ) -> Result<Vec<TimeTrackingProviderUser>, TimeTrackingError>;

    async fn list_provider_users(
        &self,
        provider: &str,
        provider_company_id: &str,
    ) -> Result<Vec<TimeTrackingProviderUser>, TimeTrackingError>;

    async fn get_provider_user(
        &self,
        provider: &str,
        provider_company_id: &str,
        provider_user_id: &str,
    ) -> Result<Option<TimeTrackingProviderUser>, TimeTrackingError>;

    async fn list_active_links(
        &self,
        provider: &str,
        provider_company_id: &str,
    ) -> Result<Vec<TimeTrackingUserLink>, TimeTrackingError>;

    async fn get_active_link_for_user(
        &self,
        user_id: &UserId,
        provider: &str,
    ) -> Result<Option<TimeTrackingUserLink>, TimeTrackingError>;

    async fn upsert_active_link(
        &self,
        link: &NewTimeTrackingUserLink,
    ) -> Result<TimeTrackingUserLink, TimeTrackingError>;

    async fn deactivate_active_link(
        &self,
        user_id: &UserId,
        provider: &str,
    ) -> Result<(), TimeTrackingError>;
}

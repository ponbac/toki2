use time::OffsetDateTime;

use super::UserId;

pub const KLEER_TIME_TRACKING_PROVIDER: &str = "kleer";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeTrackingProviderUser {
    pub id: i32,
    pub provider: String,
    pub provider_company_id: String,
    pub provider_user_id: String,
    pub foreign_id: Option<String>,
    pub internal_id: Option<String>,
    pub name: String,
    pub email: Option<String>,
    pub active: bool,
    pub last_synced_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTimeTrackingProviderUser {
    pub provider: String,
    pub provider_company_id: String,
    pub provider_user_id: String,
    pub foreign_id: Option<String>,
    pub internal_id: Option<String>,
    pub name: String,
    pub email: Option<String>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimeTrackingUserLink {
    pub id: i32,
    pub user_id: UserId,
    pub provider: String,
    pub provider_company_id: String,
    pub provider_user_id: String,
    pub provider_user_email: Option<String>,
    pub provider_user_name: Option<String>,
    pub active: bool,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub last_synced_at: OffsetDateTime,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewTimeTrackingUserLink {
    pub user_id: UserId,
    pub provider: String,
    pub provider_company_id: String,
    pub provider_user_id: String,
    pub provider_user_email: Option<String>,
    pub provider_user_name: Option<String>,
}

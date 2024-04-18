mod auth;
mod client;
mod milltime_url;
pub mod period_info;
pub mod user_calendar;

pub(crate) use milltime_url::*;

pub use auth::*;
pub use client::*;

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use std::env;

    #[test]
    fn test_create_credentials() {}
}

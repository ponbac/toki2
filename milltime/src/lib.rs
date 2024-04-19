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
    use std::env;

    #[tokio::test]
    async fn test_create_credentials() {
        dotenvy::from_filename("./milltime/.env.local").ok();
        let username = env::var("MILLTIME_USERNAME").expect("MILLTIME_USERNAME must be set");
        let password = env::var("MILLTIME_PASSWORD").expect("MILLTIME_PASSWORD must be set");

        let credentials = Credentials::new(&username, &password).await.unwrap();
        assert_eq!(credentials.username, Some(username));
        assert_eq!(credentials.csrf_token.len(), 16);
        assert!(credentials.valid_until.is_some());
    }
}

mod auth;
mod client;
mod domain;
mod milltime_url;

pub use auth::*;
pub use client::*;
pub use domain::{Day, TimeEntry, TimePeriodInfo, UserCalendar, Week};

#[cfg(test)]
mod tests {
    use tokio::sync::OnceCell;

    use super::*;
    use std::{env, error::Error};

    async fn get_credentials() -> Result<Credentials, Box<dyn Error>> {
        dotenvy::from_filename("./milltime/.env.local").ok();
        let username = env::var("MILLTIME_USERNAME").expect("MILLTIME_USERNAME must be set");
        let password = env::var("MILLTIME_PASSWORD").expect("MILLTIME_PASSWORD must be set");

        Credentials::new(&username, &password).await
    }

    static CLIENT: OnceCell<MilltimeClient> = OnceCell::const_new();

    async fn initialize_client() -> &'static MilltimeClient {
        CLIENT
            .get_or_init(|| async {
                let credentials = get_credentials()
                    .await
                    .expect("Failed to get credentials, login problem?");
                MilltimeClient::new(credentials)
            })
            .await
    }

    #[tokio::test]
    async fn test_fetch_time_period_info() {
        let client = initialize_client().await;
        let date_filter: DateFilter = "2024-01-01,2024-12-31".parse().unwrap();
        let time_period_info = client.fetch_time_period_info(date_filter).await.unwrap();

        assert_eq!(time_period_info.from.to_string(), "2024-01-01".to_string());
    }

    #[tokio::test]
    async fn test_fetch_user_calendar() {
        let client = initialize_client().await;
        let date_filter: DateFilter = "2024-04-01,2024-04-30".parse().unwrap();
        let user_calendar = client.fetch_user_calendar(date_filter).await.unwrap();

        assert_eq!(user_calendar.weeks.len(), 5);
    }
}

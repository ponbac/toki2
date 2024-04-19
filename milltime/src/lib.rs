mod auth;
mod client;
pub mod domain;
mod milltime_url;

pub(crate) use milltime_url::*;

pub use auth::*;
pub use client::*;

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    async fn get_credentials() -> Credentials {
        dotenvy::from_filename("./milltime/.env.local").ok();
        let username = env::var("MILLTIME_USERNAME").expect("MILLTIME_USERNAME must be set");
        let password = env::var("MILLTIME_PASSWORD").expect("MILLTIME_PASSWORD must be set");

        Credentials::new(&username, &password).await.unwrap()
    }

    async fn get_client() -> MilltimeClient {
        let credentials = get_credentials().await;
        MilltimeClient::new(credentials)
    }

    #[tokio::test]
    async fn test_create_credentials() {
        let credentials = get_credentials().await;

        assert_eq!(credentials.csrf_token.len(), 16);
        assert!(credentials.valid_until.is_some());
    }

    // #[tokio::test]
    // async fn test_fetch() {
    //     let client = get_client().await;
    //     let url = MilltimeURL::new().
    //     let response: MilltimeRowResponse<period_info::Period> = client.fetch(url).await.unwrap();

    //     assert_eq!(response.success, true);
    //     assert!(response.rows.len() > 0);
    // }
}

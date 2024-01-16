use std::{collections::HashMap, sync::Arc};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use az_devops::RepoClient;
use futures_util::{stream::FuturesUnordered, StreamExt};
use sqlx::PgPool;
use tokio::sync::RwLock;

use crate::domain::{RepoConfig, RepoKey};

#[derive(Debug, thiserror::Error)]
pub enum AppStateError {
    #[error("Repository client not found for: {0}")]
    RepoClientNotFound(RepoKey),
}

impl IntoResponse for AppStateError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::RepoClientNotFound(_) => StatusCode::NOT_FOUND,
        };

        (status, self.to_string()).into_response()
    }
}

#[derive(Clone)]
pub struct AppState {
    pub db_pool: Arc<PgPool>,
    repo_clients: Arc<RwLock<HashMap<RepoKey, RepoClient>>>,
}

impl AppState {
    pub async fn new(db_pool: PgPool, repo_configs: Vec<RepoConfig>) -> Self {
        let client_futures = repo_configs
            .into_iter()
            .map(|repo| async move {
                match repo.to_client().await {
                    Ok(client) => Some((repo.key(), client)),
                    Err(err) => {
                        tracing::error!(
                            "Failed to create client for repo '{}': {}",
                            repo.key(),
                            err
                        );
                        None
                    }
                }
            })
            .collect::<FuturesUnordered<_>>();

        let clients: HashMap<_, _> = client_futures
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .flatten()
            .collect();

        Self {
            db_pool: Arc::new(db_pool),
            repo_clients: Arc::new(RwLock::new(clients)),
        }
    }

    pub async fn get_repo_client(
        &self,
        key: impl Into<RepoKey>,
    ) -> Result<RepoClient, AppStateError> {
        let repo_clients = self.repo_clients.read().await;
        let key: RepoKey = key.into();

        repo_clients
            .get(&key)
            .cloned()
            .ok_or(AppStateError::RepoClientNotFound(key))
    }

    pub async fn insert_repo_client(&self, key: impl Into<RepoKey>, client: RepoClient) {
        let mut repo_clients = self.repo_clients.write().await;
        let key: RepoKey = key.into();

        repo_clients.insert(key, client);
    }
}

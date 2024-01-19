use std::{collections::HashMap, sync::Arc};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use az_devops::RepoClient;
use futures_util::{stream::FuturesUnordered, StreamExt};
use sqlx::PgPool;
use tokio::sync::RwLock;

use crate::domain::{RepoConfig, RepoDiffer, RepoKey};

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
    repos: Arc<RwLock<HashMap<RepoKey, RepoDiffer>>>,
    // todo: need to separate out the repo clients from the repo differ
}

impl AppState {
    pub async fn new(db_pool: PgPool, repo_configs: Vec<RepoConfig>) -> Self {
        let client_futures = repo_configs
            .into_iter()
            .map(|repo| async move {
                match repo.to_client().await {
                    Ok(client) => Some((repo.key(), RepoDiffer::new(repo.key(), client))),
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
            repos: Arc::new(RwLock::new(clients)),
        }
    }

    pub async fn get_repo_client(
        &self,
        key: impl Into<RepoKey>,
    ) -> Result<RepoClient, AppStateError> {
        let repo_clients = self.repos.read().await;
        let key: RepoKey = key.into();

        repo_clients
            .get(&key)
            .cloned()
            .ok_or(AppStateError::RepoClientNotFound(key))
            .map(|differ| differ.az_client)
    }

    pub async fn get_repo(&self, key: impl Into<RepoKey>) -> Result<&RepoDiffer, AppStateError> {
        let key: RepoKey = key.into();
        let repos = self.repos.write().await;

        repos
            .get(&key)
            .ok_or(AppStateError::RepoClientNotFound(key))
    }

    pub async fn insert_repo(&self, key: impl Into<RepoKey>, client: RepoClient) {
        let key: RepoKey = key.into();
        let mut repos = self.repos.write().await;

        repos.insert(key.clone(), RepoDiffer::new(key, client));
    }
}

use std::{collections::HashMap, sync::Arc};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use az_devops::{PullRequest, RepoClient};
use futures_util::{stream::FuturesUnordered, StreamExt};
use sqlx::PgPool;
use tokio::sync::{
    mpsc::{self, Sender},
    Mutex, RwLock,
};

use crate::domain::{RepoConfig, RepoDiffer, RepoDifferMessage, RepoKey};

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
    differ_txs: Arc<Mutex<HashMap<RepoKey, Sender<RepoDifferMessage>>>>,
    pr_cache: Arc<RwLock<HashMap<RepoKey, Vec<PullRequest>>>>,
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

        let pr_cache = Arc::new(RwLock::new(HashMap::new()));
        let differ_txs = clients
            .iter()
            .map(|(key, client)| {
                let mut differ = RepoDiffer::new(key.clone(), client.clone(), pr_cache.clone());

                let (tx, rx) = mpsc::channel::<RepoDifferMessage>(32);
                tokio::spawn(async move {
                    differ.run(rx).await;
                });

                (key.clone(), tx)
            })
            .collect::<HashMap<_, _>>();

        Self {
            db_pool: Arc::new(db_pool),
            repo_clients: Arc::new(RwLock::new(clients)),
            differ_txs: Arc::new(Mutex::new(differ_txs)),
            pr_cache,
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

    pub async fn get_differ_sender(
        &self,
        key: impl Into<RepoKey>,
    ) -> Result<Sender<RepoDifferMessage>, AppStateError> {
        let differs = self.differ_txs.lock().await;
        let key: RepoKey = key.into();

        differs
            .get(&key)
            .cloned()
            .ok_or(AppStateError::RepoClientNotFound(key))
    }

    pub async fn get_cached_pull_requests(
        &self,
        key: impl Into<RepoKey>,
    ) -> Result<Vec<PullRequest>, AppStateError> {
        let pr_cache = self.pr_cache.read().await;
        let key: RepoKey = key.into();

        pr_cache
            .get(&key)
            .cloned()
            .ok_or(AppStateError::RepoClientNotFound(key))
    }

    pub async fn insert_repo(&self, key: impl Into<RepoKey>, client: RepoClient) {
        let key: RepoKey = key.into();

        let mut clients = self.repo_clients.write().await;
        clients.insert(key.clone(), client.clone());

        let mut differs = self.differ_txs.lock().await;
        let (tx, rx) = mpsc::channel::<RepoDifferMessage>(32);
        let mut differ = RepoDiffer::new(key.clone(), client.clone(), self.pr_cache.clone());

        tokio::spawn(async move {
            differ.run(rx).await;
        });
        differs.insert(key, tx);
    }
}

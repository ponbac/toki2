use std::{collections::HashMap, sync::Arc, thread};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use az_devops::RepoClient;
use crossbeam::channel::{unbounded, Sender};
use futures_util::{stream::FuturesUnordered, StreamExt};
use sqlx::PgPool;
use tokio::sync::{Mutex, RwLock};

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
    differ_senders: Arc<Mutex<HashMap<RepoKey, Sender<RepoDifferMessage>>>>,
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

        let differs = clients
            .iter()
            .map(|(key, client)| {
                let (sender, reciever) = unbounded::<RepoDifferMessage>();

                let mut differ = RepoDiffer::new(key.clone(), client.clone());
                let key = key.clone();

                thread::spawn(move || {
                    differ.run(reciever);
                });

                (key, sender)
            })
            .collect::<HashMap<_, _>>();

        Self {
            db_pool: Arc::new(db_pool),
            repo_clients: Arc::new(RwLock::new(clients)),
            differ_senders: Arc::new(Mutex::new(differs)),
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

    pub async fn insert_repo(&self, key: impl Into<RepoKey>, client: RepoClient) {
        let key: RepoKey = key.into();

        let mut clients = self.repo_clients.write().await;
        clients.insert(key.clone(), client.clone());

        let mut differs = self.differ_senders.lock().await;
        let (sender, reciever) = unbounded::<RepoDifferMessage>();
        let mut differ = RepoDiffer::new(key.clone(), client.clone());

        thread::spawn(move || {
            differ.run(reciever);
        });
        differs.insert(key, sender);
    }
}

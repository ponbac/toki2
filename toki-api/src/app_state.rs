use std::{collections::HashMap, sync::Arc};

use az_devops::RepoClient;
use sqlx::PgPool;
use tokio::sync::Mutex;

use crate::domain::{RepoConfig, RepoKey};

#[derive(Clone)]
pub struct AppState {
    pub db_pool: Arc<PgPool>,
    repo_clients: Arc<Mutex<HashMap<RepoKey, RepoClient>>>,
}

impl AppState {
    pub async fn new(db_pool: PgPool, repo_configs: Vec<RepoConfig>) -> Self {
        let mut repos = HashMap::new();
        for repo in repo_configs {
            repos.insert(
                repo.key(),
                repo.to_client().await.unwrap_or_else(|_| {
                    panic!("Failed to create client for repo '{}'", repo.key())
                }),
            );
        }

        Self {
            db_pool: Arc::new(db_pool),
            repo_clients: Arc::new(Mutex::new(repos)),
        }
    }

    pub async fn get_repo_client(&self, key: impl Into<RepoKey>) -> Result<RepoClient, String> {
        let repo_clients = self.repo_clients.lock().await;
        let key: RepoKey = key.into();

        repo_clients
            .get(&key)
            .cloned()
            .ok_or_else(|| format!("Repository '{}' not found", key))
    }

    pub async fn insert_repo_client(&self, key: impl Into<RepoKey>, client: RepoClient) {
        let mut repo_clients = self.repo_clients.lock().await;
        let key: RepoKey = key.into();

        repo_clients.insert(key, client);
    }
}

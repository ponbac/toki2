use std::{collections::HashMap, sync::Arc};

use az_devops::RepoClient;
use sqlx::PgPool;
use tokio::sync::Mutex;

use crate::repository::RepoKey;

#[derive(Clone)]
pub struct AppState {
    pub db_pool: Arc<PgPool>,
    repo_clients: Arc<Mutex<HashMap<RepoKey, RepoClient>>>,
}

impl AppState {
    pub fn new(db_pool: PgPool, repo_clients: HashMap<RepoKey, RepoClient>) -> Self {
        Self {
            db_pool: Arc::new(db_pool),
            repo_clients: Arc::new(Mutex::new(repo_clients)),
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

use std::{collections::HashMap, sync::Arc};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
};
use az_devops::RepoClient;
use futures_util::{stream::FuturesUnordered, StreamExt};
use sqlx::PgPool;
use tokio::sync::{
    mpsc::{self, Sender},
    Mutex, RwLock,
};
use web_push::{IsahcWebPushClient, WebPushClient, WebPushMessage};

use crate::{
    domain::{
        NotificationHandler, PullRequest, RepoConfig, RepoDiffer, RepoDifferMessage, RepoKey,
    },
    repositories::{PushSubscriptionRepositoryImpl, RepoRepositoryImpl, UserRepositoryImpl},
};

#[derive(Debug, thiserror::Error)]
pub enum AppStateError {
    #[error("Repository client not found for: {0}")]
    RepoClientNotFound(RepoKey),
    #[error("Failed to send notification: {0}")]
    WebPushError(#[from] web_push::WebPushError),
}

impl IntoResponse for AppStateError {
    fn into_response(self) -> Response {
        let status = match self {
            Self::RepoClientNotFound(_) => StatusCode::NOT_FOUND,
            Self::WebPushError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, self.to_string()).into_response()
    }
}

#[derive(Clone)]
pub struct AppState {
    pub app_url: String,
    pub db_pool: Arc<PgPool>,
    pub user_repo: Arc<UserRepositoryImpl>,
    pub repository_repo: Arc<RepoRepositoryImpl>,
    pub push_subscriptions_repo: Arc<PushSubscriptionRepositoryImpl>,
    repo_clients: Arc<RwLock<HashMap<RepoKey, RepoClient>>>,
    differs: Arc<RwLock<HashMap<RepoKey, Arc<RepoDiffer>>>>,
    differ_txs: Arc<Mutex<HashMap<RepoKey, Sender<RepoDifferMessage>>>>,
    web_push_client: IsahcWebPushClient,
    notification_handler: Arc<NotificationHandler>,
}

impl AppState {
    pub async fn new(app_url: String, db_pool: PgPool, repo_configs: Vec<RepoConfig>) -> Self {
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

        let web_push_client = IsahcWebPushClient::new().expect("Could not create web push client");
        let notification_handler = Arc::new(NotificationHandler::new(
            db_pool.clone(),
            web_push_client.clone(),
        ));

        let mut differs = HashMap::new();
        let differ_txs = clients
            .iter()
            .map(|(key, client)| {
                let differ = Arc::new(RepoDiffer::new(
                    key.clone(),
                    client.clone(),
                    notification_handler.clone(),
                ));
                differs.insert(key.clone(), differ.clone());

                let (tx, rx) = mpsc::channel::<RepoDifferMessage>(32);
                let arced_db_pool = Arc::new(db_pool.clone());
                tokio::spawn(async move {
                    differ.run(rx, arced_db_pool.clone()).await;
                });

                (key.clone(), tx)
            })
            .collect::<HashMap<_, _>>();

        Self {
            app_url,
            db_pool: Arc::new(db_pool.clone()),
            user_repo: Arc::new(UserRepositoryImpl::new(db_pool.clone())),
            repository_repo: Arc::new(RepoRepositoryImpl::new(db_pool.clone())),
            push_subscriptions_repo: Arc::new(PushSubscriptionRepositoryImpl::new(db_pool.clone())),
            repo_clients: Arc::new(RwLock::new(clients)),
            differ_txs: Arc::new(Mutex::new(differ_txs)),
            differs: Arc::new(RwLock::new(differs)),
            web_push_client,
            notification_handler,
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

    pub async fn get_repo_differs(&self) -> Vec<Arc<RepoDiffer>> {
        let differs = self.differs.read().await;

        differs.values().cloned().collect()
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
    ) -> Result<Option<Vec<PullRequest>>, AppStateError> {
        let key: RepoKey = key.into();

        let differs = self.differs.read().await;
        let differ = differs
            .get(&key)
            .cloned()
            .ok_or(AppStateError::RepoClientNotFound(key))?;
        let cached_pull_requests = differ.prev_pull_requests.read().await.clone();

        Ok(cached_pull_requests)
    }

    pub async fn insert_repo(&self, key: impl Into<RepoKey>, client: RepoClient) {
        let key: RepoKey = key.into();

        let mut clients = self.repo_clients.write().await;
        clients.insert(key.clone(), client.clone());

        let mut differ_txs = self.differ_txs.lock().await;
        let (tx, rx) = mpsc::channel::<RepoDifferMessage>(32);
        let differ = Arc::new(RepoDiffer::new(
            key.clone(),
            client.clone(),
            self.notification_handler.clone(),
        ));
        self.differs
            .write()
            .await
            .insert(key.clone(), differ.clone());

        let db_pool = self.db_pool.clone();
        tokio::spawn(async move {
            differ.run(rx, db_pool.clone()).await;
        });
        differ_txs.insert(key, tx);
    }

    pub async fn push_notification(&self, message: WebPushMessage) -> Result<(), AppStateError> {
        self.web_push_client.send(message).await.map_err(|e| {
            tracing::error!("Failed to send notification: {:?}", e);
            AppStateError::WebPushError(e)
        })
    }
}

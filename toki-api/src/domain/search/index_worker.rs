//! Background worker for periodic search index syncing.

use std::{collections::HashMap, sync::Arc, time::Duration};

use az_devops::RepoClient;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tracing::{error, info};

use super::{
    embedder::GeminiEmbedder,
    repository::PgSearchRepository,
    source::AdoDocumentSource,
    IndexerConfig, SearchIndexer,
};
use crate::domain::RepoKey;

/// Runs a periodic search index sync across all configured repositories.
///
/// Each cycle iterates repos sequentially to avoid Gemini API rate limits.
/// Errors for individual repos are logged and skipped (non-fatal).
pub async fn run_search_index_worker(
    db_pool: Arc<PgPool>,
    repo_clients: Arc<RwLock<HashMap<RepoKey, RepoClient>>>,
    embedder: GeminiEmbedder,
    interval: Duration,
    config: IndexerConfig,
) {
    info!(
        interval_secs = interval.as_secs(),
        "Search indexer background task started"
    );

    let mut ticker = tokio::time::interval(interval);

    // Skip the first immediate tick to let the app fully start
    ticker.tick().await;

    loop {
        ticker.tick().await;

        info!("Starting search index sync cycle");

        // Snapshot repo_clients under a brief read lock
        let clients: Vec<(RepoKey, RepoClient)> = {
            let guard = repo_clients.read().await;
            guard
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };

        if clients.is_empty() {
            info!("No repositories configured, skipping sync cycle");
            continue;
        }

        let mut total_prs = 0usize;
        let mut total_work_items = 0usize;
        let mut total_errors = 0usize;

        for (key, client) in &clients {
            let source = AdoDocumentSource::new(client.clone(), key.repo_name.clone());
            let repository = PgSearchRepository::new((*db_pool).clone());
            let indexer =
                SearchIndexer::new(embedder.clone(), repository, source, config.clone());

            match indexer
                .sync_project(&key.organization, &key.project)
                .await
            {
                Ok(stats) => {
                    info!(
                        repo = %key,
                        prs = stats.prs_indexed,
                        work_items = stats.work_items_indexed,
                        deleted = stats.documents_deleted,
                        "Repo sync completed"
                    );
                    total_prs += stats.prs_indexed;
                    total_work_items += stats.work_items_indexed;
                    total_errors += stats.errors;
                }
                Err(e) => {
                    error!(repo = %key, error = %e, "Repo sync failed");
                    total_errors += 1;
                }
            }
        }

        info!(
            repos = clients.len(),
            prs = total_prs,
            work_items = total_work_items,
            errors = total_errors,
            "Search index sync cycle completed"
        );
    }
}

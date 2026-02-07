use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use tracing::instrument;

use crate::{
    domain::search::SearchResult,
    AppState,
};

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(search))
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SearchQuery {
    q: String,
    limit: Option<i32>,
}

#[instrument(name = "GET /search", skip(app_state))]
async fn search(
    State(app_state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Vec<SearchResult>>, (StatusCode, String)> {
    let search_service = app_state
        .search_service()
        .ok_or((StatusCode::SERVICE_UNAVAILABLE, "Search service not available".to_string()))?;

    let results = search_service
        .search(&query.q, query.limit)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok(Json(results))
}

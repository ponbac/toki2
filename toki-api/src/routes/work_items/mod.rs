use axum::{
    extract::{Query, State},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use tracing::instrument;

use crate::{
    adapters::inbound::http::{
        FormatForLlmResponse, IterationResponse, WorkItemProjectResponse, WorkItemResponse,
    },
    app_state::AppState,
    auth::AuthUser,
};

use super::ApiError;

// ---------------------------------------------------------------------------
// Query parameter types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectQuery {
    pub organization: String,
    pub project: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BoardQuery {
    pub organization: String,
    pub project: String,
    pub iteration_path: Option<String>,
    pub team: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FormatForLlmQuery {
    pub organization: String,
    pub project: String,
    pub work_item_id: String,
}

// ---------------------------------------------------------------------------
// Route handlers
// ---------------------------------------------------------------------------

#[instrument(name = "GET /work-items/projects")]
async fn get_projects(
    user: AuthUser,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<WorkItemProjectResponse>>, ApiError> {
    let projects = app_state
        .work_item_factory
        .get_available_projects(user.id.as_i32())
        .await?;
    Ok(Json(projects.into_iter().map(Into::into).collect()))
}

#[instrument(name = "GET /work-items/iterations")]
async fn get_iterations(
    _user: AuthUser,
    State(app_state): State<AppState>,
    Query(query): Query<ProjectQuery>,
) -> Result<Json<Vec<IterationResponse>>, ApiError> {
    let service = app_state
        .work_item_factory
        .create_service(&query.organization, &query.project)
        .await?;
    let iterations = service.get_iterations().await?;
    Ok(Json(iterations.into_iter().map(Into::into).collect()))
}

#[instrument(name = "GET /work-items/board")]
async fn get_board(
    _user: AuthUser,
    State(app_state): State<AppState>,
    Query(query): Query<BoardQuery>,
) -> Result<Json<Vec<WorkItemResponse>>, ApiError> {
    let service = app_state
        .work_item_factory
        .create_service(&query.organization, &query.project)
        .await?;
    let items = service
        .get_board_items(query.iteration_path.as_deref(), query.team.as_deref())
        .await?;
    Ok(Json(items.into_iter().map(Into::into).collect()))
}

#[instrument(name = "GET /work-items/format-for-llm")]
async fn format_for_llm(
    _user: AuthUser,
    State(app_state): State<AppState>,
    Query(query): Query<FormatForLlmQuery>,
) -> Result<Json<FormatForLlmResponse>, ApiError> {
    let service = app_state
        .work_item_factory
        .create_service(&query.organization, &query.project)
        .await?;
    let (markdown, has_images) = service
        .format_work_item_for_llm(&query.work_item_id)
        .await?;
    Ok(Json(FormatForLlmResponse {
        markdown,
        has_images,
    }))
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/projects", get(get_projects))
        .route("/iterations", get(get_iterations))
        .route("/board", get(get_board))
        .route("/format-for-llm", get(format_for_llm))
}

use std::collections::{HashMap, HashSet};

use axum::{
    extract::{Query, State},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use tracing::instrument;

use crate::{
    adapters::inbound::http::{
        BoardResponse, FormatForLlmResponse, IterationResponse, WorkItemProjectResponse,
    },
    app_state::AppState,
    auth::AuthUser,
    domain::models::WorkItem,
    routes::email::normalize_email,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveWorkItemBody {
    pub organization: String,
    pub project: String,
    pub work_item_id: String,
    pub target_column_name: String,
    pub iteration_path: Option<String>,
    pub team: Option<String>,
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
) -> Result<Json<BoardResponse>, ApiError> {
    let service = app_state
        .work_item_factory
        .create_service(&query.organization, &query.project)
        .await?;
    let mut board_data = service
        .get_board_data(query.iteration_path.as_deref(), query.team.as_deref())
        .await?;
    apply_avatar_overrides_to_work_items(&app_state, &mut board_data.items).await?;

    Ok(Json(board_data.into()))
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

#[instrument(
    name = "POST /work-items/move",
    skip(app_state, body),
    fields(
        organization = %body.organization,
        project = %body.project,
        work_item_id = %body.work_item_id,
        target_column_name = %body.target_column_name
    )
)]
async fn move_work_item(
    _user: AuthUser,
    State(app_state): State<AppState>,
    Json(body): Json<MoveWorkItemBody>,
) -> Result<StatusCode, ApiError> {
    let service = app_state
        .work_item_factory
        .create_service(&body.organization, &body.project)
        .await?;

    service
        .move_work_item_to_column(
            &body.work_item_id,
            &body.target_column_name,
            body.iteration_path.as_deref(),
            body.team.as_deref(),
        )
        .await?;

    Ok(StatusCode::NO_CONTENT)
}

async fn apply_avatar_overrides_to_work_items(
    app_state: &AppState,
    items: &mut [WorkItem],
) -> Result<(), ApiError> {
    if items.is_empty() {
        return Ok(());
    }

    let mut unique_emails = HashSet::new();
    for item in items.iter() {
        collect_work_item_person_email(&mut unique_emails, item.assigned_to.as_ref());
        collect_work_item_person_email(&mut unique_emails, item.created_by.as_ref());
    }

    if unique_emails.is_empty() {
        return Ok(());
    }

    let email_list = unique_emails.into_iter().collect::<Vec<_>>();
    let avatar_by_email = app_state
        .avatar_service
        .resolve_overrides(&email_list)
        .await?
        .into_iter()
        .map(|override_item| (override_item.email.to_lowercase(), override_item.avatar_url))
        .collect::<HashMap<_, _>>();

    for item in items.iter_mut() {
        apply_avatar_override_to_work_item_person(item.assigned_to.as_mut(), &avatar_by_email);
        apply_avatar_override_to_work_item_person(item.created_by.as_mut(), &avatar_by_email);
    }

    Ok(())
}

fn collect_work_item_person_email(
    emails: &mut HashSet<String>,
    person: Option<&crate::domain::models::WorkItemPerson>,
) {
    let Some(person) = person else {
        return;
    };

    let Some(unique_name) = person.unique_name.as_deref() else {
        return;
    };

    if let Some(email) = normalize_email(unique_name) {
        emails.insert(email);
    }
}

fn apply_avatar_override_to_work_item_person(
    person: Option<&mut crate::domain::models::WorkItemPerson>,
    avatar_by_email: &HashMap<String, String>,
) {
    let Some(person) = person else {
        return;
    };

    let Some(unique_name) = person.unique_name.as_deref() else {
        return;
    };

    let Some(email) = normalize_email(unique_name) else {
        return;
    };

    if let Some(avatar_url) = avatar_by_email.get(&email) {
        person.image_url = Some(avatar_url.clone());
    }
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
        .route("/move", post(move_work_item))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::domain::models::WorkItemPerson;
    use crate::routes::email::normalize_email;

    use super::apply_avatar_override_to_work_item_person;

    #[test]
    fn normalize_email_trims_and_lowercases() {
        assert_eq!(
            normalize_email("  USER@Example.com "),
            Some("user@example.com".to_string())
        );
        assert_eq!(normalize_email(""), None);
    }

    #[test]
    fn apply_avatar_override_to_work_item_person_updates_image_url() {
        let mut person = WorkItemPerson {
            display_name: "Test User".to_string(),
            unique_name: Some("USER@example.com".to_string()),
            image_url: Some("https://provider.example.com/avatar.png".to_string()),
        };

        let mut avatar_by_email = HashMap::new();
        avatar_by_email.insert(
            "user@example.com".to_string(),
            "https://custom.example.com/avatar.png".to_string(),
        );

        apply_avatar_override_to_work_item_person(Some(&mut person), &avatar_by_email);

        assert_eq!(
            person.image_url.as_deref(),
            Some("https://custom.example.com/avatar.png")
        );
    }
}

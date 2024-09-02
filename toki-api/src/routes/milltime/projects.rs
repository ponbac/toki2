use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use axum_extra::extract::CookieJar;
use tracing::instrument;

use crate::app_state::AppState;

use super::{CookieJarResult, MilltimeCookieJarExt};

#[instrument(name = "list_projects", skip(jar, app_state))]
pub async fn list_projects(
    State(app_state): State<AppState>,
    jar: CookieJar,
) -> CookieJarResult<Json<Vec<milltime::ProjectSearchItem>>> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    let search_filter = milltime::ProjectSearchFilter::new("Overview".to_string());
    let projects = milltime_client
        .fetch_project_search(search_filter)
        .await
        .unwrap()
        .into_iter()
        .filter(|project| project.is_member)
        .collect();

    Ok((jar, Json(projects)))
}

#[instrument(name = "list_activities", skip(jar, app_state))]
pub async fn list_activities(
    Path(project_id): Path<String>,
    State(app_state): State<AppState>,
    jar: CookieJar,
) -> CookieJarResult<Json<Vec<milltime::Activity>>> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    let activity_filter = milltime::ActivityFilter::new(
        project_id,
        "2024-04-15".to_string(),
        "2024-04-21".to_string(),
    );
    let activities = milltime_client
        .fetch_activities(activity_filter)
        .await
        .unwrap();

    Ok((jar, Json(activities)))
}

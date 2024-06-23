use crate::{auth, repositories::MilltimeRepository};
use std::ops::Add;

use axum::{
    debug_handler,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{delete, get, post, put},
    Json, Router,
};
use axum_extra::extract::{cookie::Cookie, CookieJar};
use serde::Deserialize;
use time::{Duration, OffsetDateTime};
use tracing::instrument;

use crate::{app_state::AppState, auth::AuthSession, domain::MilltimePassword, repositories};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/authenticate", post(authenticate))
        .route("/projects", get(list_projects))
        .route("/projects/:project_id/activities", get(list_activities))
        .route("/time-info", get(get_time_info))
        .route("/timer", get(get_timer))
        .route("/timer", post(start_timer))
        .route("/timer", delete(stop_timer))
        .route("/timer", put(save_timer))
}

type CookieJarResult<T> = Result<(CookieJar, T), (StatusCode, String)>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AuthenticatePayload {
    username: String,
    password: String,
}

#[instrument(name = "authenticate", skip(jar, app_state))]
#[debug_handler]
async fn authenticate(
    State(app_state): State<AppState>,
    jar: CookieJar,
    Json(body): Json<AuthenticatePayload>,
) -> CookieJarResult<StatusCode> {
    let credentials = milltime::Credentials::new(&body.username, &body.password).await;
    match credentials {
        Ok(creds) => {
            let domain = app_state.cookie_domain;
            let encrypted_password = MilltimePassword::new(body.password.clone()).to_encrypted();
            let mut jar = jar
                .add(
                    Cookie::build(("mt_user", body.username))
                        .domain(domain.clone())
                        .path("/")
                        .secure(true)
                        .http_only(false)
                        .expires(OffsetDateTime::now_utc().add(Duration::days(30)))
                        .build(),
                )
                .add(
                    Cookie::build(("mt_password", encrypted_password))
                        .domain(domain.clone())
                        .path("/")
                        .secure(true)
                        .http_only(true)
                        .expires(OffsetDateTime::now_utc().add(Duration::days(30)))
                        .build(),
                );
            jar = jar.with_milltime_credentials(&creds, &domain);

            Ok((jar, StatusCode::OK))
        }
        Err(_) => Err((StatusCode::BAD_REQUEST, "Invalid credentials".to_string())),
    }
}

#[instrument(name = "list_projects", skip(jar, app_state))]
async fn list_projects(
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
async fn list_activities(
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

#[derive(Debug, Deserialize)]
struct DateFilterQuery {
    from: String,
    to: String,
}

#[instrument(name = "get_time_info", skip(jar, app_state))]
async fn get_time_info(
    jar: CookieJar,
    State(app_state): State<AppState>,
    Query(date_filter): Query<DateFilterQuery>,
) -> CookieJarResult<Json<milltime::TimeInfo>> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    let date_filter: milltime::DateFilter = format!("{},{}", date_filter.from, date_filter.to)
        .parse()
        .map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "could not parse date range".to_string(),
            )
        })?;
    let time_info = milltime_client
        .fetch_time_info(date_filter)
        .await
        .map_err(|_| (StatusCode::OK, "".to_string()))?;

    Ok((jar, Json(time_info)))
}

#[instrument(name = "get_timer", skip(jar, app_state, auth_session))]
async fn get_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> CookieJarResult<Json<milltime::TimerRegistration>> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    let mt_timer = milltime_client.fetch_timer().await;

    let milltime_repo = app_state.milltime_repo.clone();
    let user = auth_session.user.expect("user not found");
    let active_timer = milltime_repo.active_timer(&user.id).await;

    match (mt_timer, active_timer) {
        (Ok(mt_timer), Ok(Some(_))) => Ok((jar, Json(mt_timer))),
        (Ok(mt_timer), Ok(None)) => {
            tracing::warn!("milltime timer found but no active timer in db");
            Ok((jar, Json(mt_timer)))
        }
        (Ok(mt_timer), Err(e)) => {
            tracing::error!("failed to fetch single active timer in db: {:?}", e);
            Ok((jar, Json(mt_timer)))
        }
        (Err(e), Ok(Some(_))) => {
            tracing::error!("failed to fetch milltime timer, but found in db: {:?}", e);
            app_state
                .milltime_repo
                .delete_active_timer(&user.id)
                .await
                .unwrap();

            Err((StatusCode::OK, "".to_string()))
        }
        _ => Err((StatusCode::OK, "".to_string())),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartTimerPayload {
    activity: String,
    activity_name: String,
    project_id: String,
    project_name: String,
    user_note: Option<String>,
    reg_day: String,
    week_number: i64,
}

#[instrument(name = "start_timer", skip(jar, app_state, auth_session))]
async fn start_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
    Json(body): Json<StartTimerPayload>,
) -> CookieJarResult<StatusCode> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    let start_timer_options = milltime::StartTimerOptions::new(
        body.activity.clone(),
        body.activity_name.clone(),
        body.project_id.clone(),
        body.project_name.clone(),
        milltime_client.user_id().to_string(),
        body.user_note.clone(),
        body.reg_day.clone(),
        body.week_number,
    );

    milltime_client
        .start_timer(start_timer_options)
        .await
        .unwrap();

    let milltime_repo = app_state.milltime_repo.clone();
    let user = auth_session.user.expect("user not found");
    let new_timer = repositories::NewMilltimeTimer {
        user_id: user.id,
        start_time: time::OffsetDateTime::now_utc(),
        project_id: body.project_id.clone(),
        activity_id: body.activity.clone(),
    };

    if let Err(e) = milltime_repo.create_timer(&new_timer).await {
        tracing::error!("failed to create timer: {:?}", e);
    }

    Ok((jar, StatusCode::OK))
}

#[instrument(name = "stop_timer", skip(jar, app_state, auth_session))]
async fn stop_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> CookieJarResult<StatusCode> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    milltime_client.stop_timer().await.unwrap();

    let milltime_repo = app_state.milltime_repo.clone();
    let user = auth_session.user.expect("user not found");
    if let Err(e) = milltime_repo.delete_active_timer(&user.id).await {
        tracing::error!("failed to delete active timer: {:?}", e);
    }

    Ok((jar, StatusCode::OK))
}

#[instrument(name = "save_timer", skip(jar, app_state, auth_session))]
async fn save_timer(
    jar: CookieJar,
    auth_session: AuthSession,
    State(app_state): State<AppState>,
) -> CookieJarResult<StatusCode> {
    let (milltime_client, jar) = jar.into_milltime_client(&app_state.cookie_domain).await?;

    milltime_client.save_timer().await.unwrap();

    let milltime_repo = app_state.milltime_repo.clone();
    let user = auth_session.user.expect("user not found");
    let end_time = time::OffsetDateTime::now_utc();
    if let Err(e) = milltime_repo.save_active_timer(&user.id, &end_time).await {
        tracing::error!("failed to save active timer: {:?}", e);
    }

    Ok((jar, StatusCode::OK))
}

trait MilltimeCookieJarExt: std::marker::Sized {
    async fn into_milltime_client(
        self,
        domain: &str,
    ) -> Result<(milltime::MilltimeClient, Self), (StatusCode, String)>;
    fn with_milltime_credentials(self, credentials: &milltime::Credentials, domain: &str) -> Self;
}

impl MilltimeCookieJarExt for CookieJar {
    async fn into_milltime_client(
        self,
        domain: &str,
    ) -> Result<(milltime::MilltimeClient, Self), (StatusCode, String)> {
        let (credentials, jar) = match self.clone().try_into() {
            Ok(c) => {
                tracing::debug!("using existing milltime credentials");
                (c, self)
            }
            Err(_) => {
                let user = self.get("mt_user").ok_or((
                    StatusCode::UNAUTHORIZED,
                    "missing mt_user cookie".to_string(),
                ))?;
                let pass = self.get("mt_password").ok_or((
                    StatusCode::UNAUTHORIZED,
                    "missing mt_password cookie".to_string(),
                ))?;
                let decrypted_pass = MilltimePassword::from_encrypted(pass.value().to_string());
                let creds = milltime::Credentials::new(user.value(), decrypted_pass.as_ref())
                    .await
                    .map_err(|e| {
                        tracing::error!("failed to create milltime credentials: {:?}", e);
                        (StatusCode::UNAUTHORIZED, e.to_string())
                    })?;
                let jar = self.with_milltime_credentials(&creds, domain);

                tracing::debug!("created new milltime credentials");
                (creds, jar)
            }
        };

        Ok((milltime::MilltimeClient::new(credentials), jar))
    }

    fn with_milltime_credentials(self, credentials: &milltime::Credentials, domain: &str) -> Self {
        let mut jar = self.clone();
        for cookie in credentials.auth_cookies(domain.to_string()) {
            jar = jar.add(cookie);
        }

        jar
    }
}

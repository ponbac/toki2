mod authenticate;
mod calendar;
mod projects;
mod timer;

use axum::{
    routing::{get, post, put},
    Router,
};
use axum_extra::extract::CookieJar;

use super::ApiError;
use crate::app_state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/authenticate", post(authenticate::authenticate))
        .route("/projects", get(projects::list_projects))
        .route(
            "/projects/:project_id/activities",
            get(projects::list_activities),
        )
        .route("/time-info", get(calendar::get_time_info))
        .route(
            "/time-entries",
            get(calendar::get_time_entries)
                .put(calendar::edit_project_registration)
                .delete(calendar::delete_project_registration)
                .post(calendar::create_project_registration),
        )
        .route("/timer-history", get(timer::get_timer_history))
        .route(
            "/timer",
            get(timer::get_timer)
                .post(timer::start_timer)
                .delete(timer::stop_timer)
                .put(timer::save_timer),
        )
        .route("/update-timer", put(timer::edit_timer))
}

type CookieJarResult<T> = Result<(CookieJar, T), ApiError>;

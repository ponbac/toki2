mod absences;
mod admin;
mod calendar;
mod connection;
mod projects;
mod timer;

use axum::{
    routing::{get, post, put},
    Router,
};

use crate::app_state::AppState;

fn normalize_user_note(note: impl AsRef<str>) -> String {
    note.as_ref().trim().to_owned()
}

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/admin", admin::router())
        .route("/connection", get(connection::connection_status))
        .route("/projects", get(projects::list_projects))
        .route(
            "/projects/:project_id/activities",
            get(projects::list_activities),
        )
        .route("/time-info", get(calendar::get_time_info))
        .route(
            "/absences",
            get(absences::get_absences)
                .post(absences::create_absences)
                .delete(absences::delete_absence),
        )
        .route(
            "/absence-day-defaults",
            get(absences::get_absence_day_defaults),
        )
        .route("/absence-types", get(absences::get_absence_types))
        .route("/absence-children", get(absences::get_absence_children))
        .route(
            "/time-entry-day-statuses",
            get(calendar::get_time_entry_day_statuses),
        )
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
            post(timer::start_timer)
                .delete(timer::stop_timer)
                .put(timer::save_timer),
        )
        .route("/update-timer", put(timer::edit_timer))
}

pub(crate) use timer::get_timer as get_timer_status;

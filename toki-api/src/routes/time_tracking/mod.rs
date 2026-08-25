mod absences;
mod admin;
mod calendar;
mod connection;
mod projects;
mod timer;

use axum::{routing::get, Router};

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
            "/projects/{project_id}/activities",
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
            get(calendar::get_time_entries).post(calendar::create_time_entry),
        )
        .route(
            "/time-entries/{registration_id}",
            axum::routing::put(calendar::update_time_entry).delete(calendar::delete_time_entry),
        )
        .route("/timer-history", get(timer::get_timer_history))
        .route(
            "/timer",
            get(timer::get_timer)
                .post(timer::start_timer)
                .delete(timer::stop_timer)
                .patch(timer::edit_timer),
        )
        .route("/timer/save", axum::routing::post(timer::save_timer))
}

pub(crate) use calendar::{
    __path_create_time_entry, __path_delete_time_entry, __path_get_time_entries,
    __path_get_time_entry_day_statuses, __path_get_time_info, __path_update_time_entry,
};
pub(crate) use connection::__path_connection_status;
pub(crate) use projects::{__path_list_activities, __path_list_projects};
pub(crate) use timer::{
    __path_edit_timer, __path_get_timer, __path_get_timer_history, __path_save_timer,
    __path_start_timer, __path_stop_timer,
};

use crate::app::App;
use crate::config::TokiConfig;
use crate::types::{Activity, Project, TimeEntry};
use time::OffsetDateTime;

pub fn test_config() -> TokiConfig {
    TokiConfig::default()
}

pub fn test_app() -> App {
    App::new(1, &test_config())
}

#[allow(dead_code)]
pub fn project(id: &str, name: &str) -> Project {
    Project {
        id: id.to_string(),
        name: name.to_string(),
    }
}

#[allow(dead_code)]
pub fn activity(id: &str, project_id: &str, name: &str) -> Activity {
    Activity {
        id: id.to_string(),
        project_id: project_id.to_string(),
        name: name.to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
pub fn time_entry(
    registration_id: &str,
    project_id: &str,
    project_name: &str,
    activity_id: &str,
    activity_name: &str,
    date: &str,
    hours: f64,
    note: Option<&str>,
    start_time: Option<OffsetDateTime>,
    end_time: Option<OffsetDateTime>,
) -> TimeEntry {
    TimeEntry {
        registration_id: registration_id.to_string(),
        project_id: project_id.to_string(),
        project_name: project_name.to_string(),
        activity_id: activity_id.to_string(),
        activity_name: activity_name.to_string(),
        date: date.to_string(),
        hours,
        note: note.map(ToString::to_string),
        start_time,
        end_time,
        week_number: 1,
    }
}

#[test]
fn app_defaults_to_timer_view() {
    let app = test_app();

    assert!(app.running);
    assert_eq!(app.current_view, crate::app::View::Timer);
    assert_eq!(app.timer_state, crate::app::TimerState::Stopped);
}

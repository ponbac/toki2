use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

/// A project available for time tracking, derived from timer history.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
}

/// An activity belonging to a project.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Activity {
    pub id: String,
    pub name: String,
    pub project_id: String,
}

/// A timer history entry as returned by toki-api.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimerHistoryEntry {
    pub id: i32,
    pub user_id: i32,
    #[serde(with = "time::serde::rfc3339")]
    pub start_time: OffsetDateTime,
    #[serde(with = "time::serde::rfc3339::option")]
    pub end_time: Option<OffsetDateTime>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub activity_id: Option<String>,
    pub activity_name: Option<String>,
    pub note: Option<String>,
}

/// The current user, as returned by GET /me.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    pub id: i32,
    pub email: String,
    pub full_name: String,
}

/// Build Project + Activity lists from a history entry list.
/// Extracts DISTINCT project/activity combinations from history.
pub fn build_projects_activities(history: &[TimerHistoryEntry]) -> (Vec<Project>, Vec<Activity>) {
    let mut projects: Vec<Project> = Vec::new();
    let mut activities: Vec<Activity> = Vec::new();

    for entry in history {
        let (Some(pid), Some(pname), Some(aid), Some(aname)) = (
            entry.project_id.as_ref(),
            entry.project_name.as_ref(),
            entry.activity_id.as_ref(),
            entry.activity_name.as_ref(),
        ) else {
            continue;
        };

        if !projects.iter().any(|p: &Project| &p.id == pid) {
            projects.push(Project {
                id: pid.clone(),
                name: pname.clone(),
            });
        }
        if !activities
            .iter()
            .any(|a: &Activity| &a.id == aid && &a.project_id == pid)
        {
            activities.push(Activity {
                id: aid.clone(),
                name: aname.clone(),
                project_id: pid.clone(),
            });
        }
    }

    // Sort for stable display
    projects.sort_by(|a, b| a.name.cmp(&b.name));
    activities.sort_by(|a, b| a.project_id.cmp(&b.project_id).then(a.name.cmp(&b.name)));

    (projects, activities)
}

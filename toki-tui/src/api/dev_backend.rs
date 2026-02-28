use crate::types::{Activity, Project, TimeEntry};
use std::sync::{Arc, Mutex};
use time::macros::offset;
use time::OffsetDateTime;

#[derive(Debug, Clone)]
pub struct DevBackend {
    store: Arc<Mutex<Vec<DevEntry>>>,
}

#[derive(Debug, Clone)]
struct DevEntry {
    registration_id: String,
    start_time: OffsetDateTime,
    end_time: Option<OffsetDateTime>,
    project_id: Option<String>,
    project_name: Option<String>,
    activity_id: Option<String>,
    activity_name: Option<String>,
    note: Option<String>,
}

impl DevBackend {
    pub fn new() -> Self {
        Self {
            store: Arc::new(Mutex::new(seed_dev_history())),
        }
    }

    pub fn time_entries(&self) -> Vec<TimeEntry> {
        let store = self.store.lock().expect("dev store lock poisoned").clone();
        store
            .iter()
            .map(|e| TimeEntry {
                registration_id: e.registration_id.clone(),
                project_id: e.project_id.clone().unwrap_or_default(),
                project_name: e.project_name.clone().unwrap_or_default(),
                activity_id: e.activity_id.clone().unwrap_or_default(),
                activity_name: e.activity_name.clone().unwrap_or_default(),
                date: {
                    let d = e.start_time.date();
                    format!("{:04}-{:02}-{:02}", d.year(), d.month() as u8, d.day())
                },
                hours: e
                    .end_time
                    .map(|end| (end - e.start_time).whole_seconds() as f64 / 3600.0)
                    .unwrap_or(0.0),
                note: e.note.clone(),
                start_time: Some(e.start_time),
                end_time: e.end_time,
                week_number: e.start_time.iso_week(),
            })
            .collect()
    }

    pub fn delete_entry(&self, registration_id: &str) {
        self.store
            .lock()
            .expect("dev store lock poisoned")
            .retain(|entry| entry.registration_id != registration_id);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn edit_entry(
        &self,
        registration_id: &str,
        project_id: &str,
        project_name: &str,
        activity_id: &str,
        activity_name: &str,
        start_time: OffsetDateTime,
        end_time: OffsetDateTime,
        user_note: &str,
    ) {
        if let Some(entry) = self
            .store
            .lock()
            .expect("dev store lock poisoned")
            .iter_mut()
            .find(|entry| entry.registration_id == registration_id)
        {
            entry.project_id = Some(project_id.to_string());
            entry.project_name = Some(project_name.to_string());
            entry.activity_id = Some(activity_id.to_string());
            entry.activity_name = Some(activity_name.to_string());
            entry.start_time = start_time;
            entry.end_time = Some(end_time);
            entry.note = Some(user_note.to_string());
        }
    }

    pub fn projects(&self) -> Vec<Project> {
        vec![
            Project {
                id: "proj_1".to_string(),
                name: "Nordic Crisis Manager".to_string(),
            },
            Project {
                id: "proj_2".to_string(),
                name: "Azure DevOps Integration".to_string(),
            },
            Project {
                id: "proj_3".to_string(),
                name: "TUI Development".to_string(),
            },
        ]
    }

    pub fn activities(&self, project_id: &str) -> Vec<Activity> {
        vec![
            Activity {
                id: "act_1_1".to_string(),
                name: "Backend Development".to_string(),
                project_id: project_id.to_string(),
            },
            Activity {
                id: "act_1_4".to_string(),
                name: "Code Review".to_string(),
                project_id: project_id.to_string(),
            },
        ]
    }

    pub fn time_info(&self) -> crate::types::TimeInfo {
        crate::types::TimeInfo {
            period_time_left: 6.0,
            worked_period_time: 26.0,
            scheduled_period_time: 32.0,
            worked_period_with_absence_time: 26.0,
            flex_time_current: 1.5,
        }
    }
}

fn seed_dev_history() -> Vec<DevEntry> {
    let now = OffsetDateTime::now_utc().to_offset(offset!(+1));
    let today = now.date();

    let entry = |idx: u32,
                 h_start: u8,
                 h_end: u8,
                 pid: &str,
                 pname: &str,
                 aid: &str,
                 aname: &str,
                 note: &str| {
        let start = OffsetDateTime::new_in_offset(
            today,
            time::Time::from_hms(h_start, 0, 0).expect("valid hour"),
            offset!(+1),
        );
        let end = OffsetDateTime::new_in_offset(
            today,
            time::Time::from_hms(h_end, 0, 0).expect("valid hour"),
            offset!(+1),
        );

        DevEntry {
            registration_id: format!("dev-reg-{}", idx),
            start_time: start,
            end_time: Some(end),
            project_id: Some(pid.to_string()),
            project_name: Some(pname.to_string()),
            activity_id: Some(aid.to_string()),
            activity_name: Some(aname.to_string()),
            note: Some(note.to_string()),
        }
    };

    vec![
        entry(
            1,
            8,
            10,
            "proj_1",
            "Nordic Crisis Manager",
            "act_1_1",
            "Backend Development",
            "API refactor",
        ),
        entry(
            2,
            10,
            12,
            "proj_1",
            "Nordic Crisis Manager",
            "act_1_4",
            "Code Review",
            "PR review",
        ),
        entry(
            3,
            13,
            15,
            "proj_2",
            "Azure DevOps Integration",
            "act_2_1",
            "API Integration",
            "Webhook setup",
        ),
        entry(
            4,
            15,
            17,
            "proj_3",
            "TUI Development",
            "act_3_2",
            "Feature Implementation",
            "Scrollable lists",
        ),
    ]
}

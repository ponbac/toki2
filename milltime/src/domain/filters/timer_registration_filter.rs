use crate::domain::TimerRegistrationPayload;

use super::MilltimeFilter;

pub struct TimerRegistrationFilter {
    user_id: String,
    project_id: String,
    activity: String,
}

impl TimerRegistrationFilter {
    pub fn new(user_id: String, project_id: String, activity: String) -> Self {
        Self {
            user_id,
            project_id,
            activity,
        }
    }
}

impl MilltimeFilter for TimerRegistrationFilter {
    fn as_milltime_filter(&self) -> String {
        format!(
            r#"[["UserId","=","{}"],["ProjectId","=","{}"],["Activity","=","{}"]]"#,
            self.user_id, self.project_id, self.activity
        )
    }
}

impl From<&TimerRegistrationPayload> for TimerRegistrationFilter {
    fn from(payload: &TimerRegistrationPayload) -> Self {
        Self {
            user_id: payload.userid.clone(),
            project_id: payload.projectid.clone(),
            activity: payload.activity.clone(),
        }
    }
}

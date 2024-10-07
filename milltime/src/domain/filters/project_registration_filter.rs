use super::MilltimeFilter;

pub struct ProjectRegistrationFilter {
    project_registration_id: String,
}

impl ProjectRegistrationFilter {
    pub fn new(project_registration_id: String) -> Self {
        Self {
            project_registration_id,
        }
    }
}

impl MilltimeFilter for ProjectRegistrationFilter {
    fn as_milltime_filter(&self) -> String {
        format!(
            r#"[["ProjectRegistrationId","=","{}"]]"#,
            self.project_registration_id
        )
    }
}

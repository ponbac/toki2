use super::MilltimeFilter;

pub struct ProjectRegistrationDeleteFilter {
    project_registration_id: String,
    time_distribution_type: String,
}

impl ProjectRegistrationDeleteFilter {
    pub fn new(project_registration_id: String) -> Self {
        Self {
            project_registration_id,
            time_distribution_type: "NORMALTIME".to_string(),
        }
    }

    pub fn with_time_distribution_type(mut self, time_distribution_type: String) -> Self {
        self.time_distribution_type = time_distribution_type;
        self
    }
}

impl MilltimeFilter for ProjectRegistrationDeleteFilter {
    fn as_milltime_filter(&self) -> String {
        format!(
            r#"[["ProjectRegistrationId","=","{}"],["TimeDistributionType","=","{}"]]"#,
            self.project_registration_id, self.time_distribution_type
        )
    }
}

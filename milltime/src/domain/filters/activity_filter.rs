use super::MilltimeFilter;

pub struct ActivityFilter {
    project_id: String,
    from_date: String,
    to_date: String,
}

impl ActivityFilter {
    pub fn new(project_id: String, from_date: String, to_date: String) -> Self {
        Self {
            project_id,
            from_date,
            to_date,
        }
    }
}

impl MilltimeFilter for ActivityFilter {
    fn as_milltime_filter(&self) -> String {
        format!(
            r#"[["ProjectId","=","{}"],["fromDate","=","{}"],["toDate","=","{}"]]"#,
            self.project_id, self.from_date, self.to_date
        )
    }
}

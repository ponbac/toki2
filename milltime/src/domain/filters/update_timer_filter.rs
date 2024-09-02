use super::MilltimeFilter;

pub struct UpdateTimerFilter {
    user_note: String,
    updated: String,
}

impl UpdateTimerFilter {
    pub fn new(user_note: String) -> Self {
        Self {
            user_note,
            updated: "true".to_string(),
        }
    }
}

impl MilltimeFilter for UpdateTimerFilter {
    fn as_milltime_filter(&self) -> String {
        format!(
            r#"[["UserNote","=","{}"],["Updated","=","{}"]]"#,
            self.user_note, self.updated
        )
    }
}

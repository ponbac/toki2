use super::MilltimeFilter;

pub struct ProjectSearchFilter {
    view: String,
}

impl ProjectSearchFilter {
    pub fn new(view: String) -> Self {
        Self { view }
    }
}

impl MilltimeFilter for ProjectSearchFilter {
    fn as_milltime_filter(&self) -> String {
        format!(r#"[["View","=","{}"]]"#, self.view)
    }
}

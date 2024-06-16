use std::env;

use crate::domain::MilltimeFilter;

#[derive(Debug)]
pub struct MilltimeURL(String);

impl AsRef<str> for MilltimeURL {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl MilltimeURL {
    /// Creates a new MilltimeURL from the environment variable `MILLTIME_URL`.
    pub fn from_env() -> Self {
        Self(env::var("MILLTIME_URL").expect("MILLTIME_URL must be set in env"))
    }

    /// Append the given path to the URL.
    pub fn append_path(&self, path: &str) -> Self {
        let trimmed_url = self.0.trim_end_matches('/');
        let trimmed_path = path.trim_start_matches('/');
        Self(format!("{}/{}", trimmed_url, trimmed_path))
    }

    pub fn with_filter(&self, filter: &impl MilltimeFilter) -> Self {
        if self.0.contains('?') {
            Self(format!("{}&filter={}", self.0, filter.as_milltime_filter()))
        } else {
            Self(format!("{}?filter={}", self.0, filter.as_milltime_filter()))
        }
    }
}

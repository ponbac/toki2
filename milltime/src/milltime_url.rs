use std::env;

use chrono::NaiveDate;

#[derive(Debug)]
pub struct MilltimeURL(String);

impl AsRef<str> for MilltimeURL {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Default for MilltimeURL {
    fn default() -> Self {
        Self::new()
    }
}

impl MilltimeURL {
    /// Creates a new MilltimeURL from the environment variable `MILLTIME_URL`.
    pub fn new() -> Self {
        Self(env::var("MILLTIME_URL").expect("MILLTIME_URL must be set in env"))
    }

    /// Append the given path to the URL.
    pub fn append_path(&self, path: &str) -> Self {
        match self.0.chars().last() {
            Some('/') => Self(format!("{}{}", self.0, path)),
            _ => Self(format!("{}/{}", self.0, path)),
        }
    }

    /// Adds query parameter `filter` to the URL, containing a from and to date.
    pub fn with_date_filter(&self, from: &NaiveDate, to: &NaiveDate) -> Self {
        let filter_value = format!(
            "[[\"fromDate\",\"=\",\"{}\"],[\"toDate\",\"=\",\"{}\"]]",
            from, to
        );

        if self.0.contains('?') {
            Self(format!("{}&filter={}", self.0, filter_value))
        } else {
            Self(format!("{}?filter={}", self.0, filter_value))
        }
    }
}

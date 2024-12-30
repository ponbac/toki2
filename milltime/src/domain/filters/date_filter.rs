use std::str::FromStr;

use super::MilltimeFilter;

/// A filter for a date range.
///
/// The easiest way to create a `DateFilter` is to parse a string in the format `YYYY-MM-DD,YYYY-MM-DD`.
///
/// ```rust
/// # use milltime::DateFilter;
/// # fn main() -> Result<(), chrono::ParseError> {
/// let date_filter = "2024-01-01,2024-01-31".parse::<DateFilter>()?;
/// # assert_eq!(date_filter.from, chrono::NaiveDate::from_ymd(2024, 1, 1));
/// # assert_eq!(date_filter.to, chrono::NaiveDate::from_ymd(2024, 1, 31));
/// # Ok(())
/// # }
/// ```
pub struct DateFilter {
    pub from: chrono::NaiveDate,
    pub to: chrono::NaiveDate,
}

impl DateFilter {
    pub fn new(from: chrono::NaiveDate, to: chrono::NaiveDate) -> Self {
        Self { from, to }
    }
}

impl MilltimeFilter for DateFilter {
    fn as_milltime_filter(&self) -> String {
        format!(
            r#"[["fromDate","=","{}"],["toDate","=","{}"]]"#,
            self.from, self.to
        )
    }
}

impl FromStr for DateFilter {
    type Err = chrono::ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts: Vec<&str> = s.split(',').collect();
        let from = chrono::NaiveDate::parse_from_str(parts[0], "%Y-%m-%d")?;
        let to = chrono::NaiveDate::parse_from_str(parts[1], "%Y-%m-%d")?;

        Ok(Self { from, to })
    }
}

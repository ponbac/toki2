use std::str::FromStr;

use super::MilltimeFilter;

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

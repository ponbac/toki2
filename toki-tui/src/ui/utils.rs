use ratatui::layout::{Constraint, Direction, Layout, Rect};

pub fn to_local_time(dt: time::OffsetDateTime) -> time::OffsetDateTime {
    crate::time_utils::to_local_time(dt)
}

/// Helper function to create a centered rectangle
pub fn centered_rect(width: u16, height: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length((r.height.saturating_sub(height)) / 2),
            Constraint::Length(height),
            Constraint::Length((r.height.saturating_sub(height)) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length((r.width.saturating_sub(width)) / 2),
            Constraint::Length(width),
            Constraint::Length((r.width.saturating_sub(width)) / 2),
        ])
        .split(popup_layout[1])[1]
}

/// Parses a "YYYY-MM-DD" date string and returns the English weekday name.
/// Returns "Unknown" if parsing fails.
pub fn parse_date_weekday(date_str: &str) -> &'static str {
    let parts: Vec<&str> = date_str.splitn(3, '-').collect();
    if parts.len() != 3 {
        return "Unknown";
    }
    let (Ok(year), Ok(month_u8), Ok(day)) = (
        parts[0].parse::<i32>(),
        parts[1].parse::<u8>(),
        parts[2].parse::<u8>(),
    ) else {
        return "Unknown";
    };
    let Ok(month) = time::Month::try_from(month_u8) else {
        return "Unknown";
    };
    let Ok(date) = time::Date::from_calendar_date(year, month, day) else {
        return "Unknown";
    };
    match date.weekday() {
        time::Weekday::Monday => "Monday",
        time::Weekday::Tuesday => "Tuesday",
        time::Weekday::Wednesday => "Wednesday",
        time::Weekday::Thursday => "Thursday",
        time::Weekday::Friday => "Friday",
        time::Weekday::Saturday => "Saturday",
        time::Weekday::Sunday => "Sunday",
    }
}

/// Formats a duration in hours (f64) as "HHh:MMm", e.g. 6.5 → "06h:30m".
pub fn format_hours_hm(hours: f64) -> String {
    let total_minutes = (hours * 60.0).round() as u64;
    let h = total_minutes / 60;
    let m = total_minutes % 60;
    format!("{:02}h:{:02}m", h, m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_hours_hm() {
        assert_eq!(format_hours_hm(0.0), "00h:00m");
        assert_eq!(format_hours_hm(6.5), "06h:30m");
        assert_eq!(format_hours_hm(1.0 / 60.0), "00h:01m"); // 1 minute
        assert_eq!(format_hours_hm(10.0), "10h:00m");
    }
}

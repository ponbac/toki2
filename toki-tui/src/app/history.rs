use super::*;

impl App {
    /// Build the history list entries (indices into timer_history)
    pub fn rebuild_history_list(&mut self) {
        let month_ago = OffsetDateTime::now_utc() - time::Duration::days(30);
        self.history_list_entries = self
            .timer_history
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.start_time >= month_ago)
            .map(|(idx, _)| idx)
            .collect();
    }

    /// Compute overlapping time entries per day
    pub(super) fn compute_overlaps(&mut self) {
        self.overlapping_entry_ids.clear();

        use std::collections::HashMap;
        let mut entries_by_date: HashMap<time::Date, Vec<&TimerHistoryEntry>> = HashMap::new();

        for entry in &self.timer_history {
            let date = entry.start_time.date();
            entries_by_date.entry(date).or_default().push(entry);
        }

        for (_, day_entries) in entries_by_date {
            if day_entries.len() < 2 {
                continue;
            }

            let mut intervals: Vec<(i32, i64, i64)> = day_entries
                .iter()
                .filter_map(|entry| {
                    let end = entry.end_time?;
                    let start_mins = entry.start_time.time().hour() as i64 * 60
                        + entry.start_time.time().minute() as i64;
                    let end_mins = end.time().hour() as i64 * 60 + end.time().minute() as i64;
                    Some((entry.id, start_mins, end_mins))
                })
                .collect();

            intervals.sort_by_key(|(_, start, _)| *start);

            for (i, (_, _, curr_end)) in intervals.iter().enumerate() {
                for (_, next_start, _) in intervals.iter().skip(i + 1) {
                    if *next_start < *curr_end {
                        self.overlapping_entry_ids.insert(intervals[i].0);
                        if let Some((id, _, _)) = intervals
                            .iter()
                            .skip(i + 1)
                            .find(|(_, s, _)| *s < *curr_end)
                        {
                            self.overlapping_entry_ids.insert(*id);
                        }
                    } else {
                        break;
                    }
                }
            }
        }
    }

    /// Check if an entry has overlapping times
    pub fn is_entry_overlapping(&self, entry_id: i32) -> bool {
        self.overlapping_entry_ids.contains(&entry_id)
    }

    pub(super) fn week_start(dt: OffsetDateTime) -> OffsetDateTime {
        let weekday = dt.weekday();
        let days_since_monday = weekday.number_days_from_monday();
        let monday = dt - time::Duration::days(days_since_monday as i64);
        monday.replace_time(time::Time::MIDNIGHT)
    }

    pub(super) fn week_end(dt: OffsetDateTime) -> OffsetDateTime {
        let weekday = dt.weekday();
        let days_until_sunday = 6 - weekday.number_days_from_monday();
        let sunday = dt + time::Duration::days(days_until_sunday as i64);
        sunday.replace_time(time::Time::MIDNIGHT) + time::Duration::nanoseconds(86_399_999_999_999)
    }

    /// Get this week's history entries (Monday to Sunday)
    pub fn this_week_history(&self) -> Vec<&TimerHistoryEntry> {
        let now = OffsetDateTime::now_utc();
        let week_start = Self::week_start(now);
        let week_end = Self::week_end(now);
        self.timer_history
            .iter()
            .filter(|entry| entry.start_time >= week_start && entry.start_time <= week_end)
            .collect()
    }

    /// Get history entries from the last month (for History view)
    #[allow(dead_code)]
    pub fn last_month_history(&self) -> Vec<&TimerHistoryEntry> {
        let now = OffsetDateTime::now_utc();
        let month_ago = now - time::Duration::days(30);
        self.timer_history
            .iter()
            .filter(|entry| entry.start_time >= month_ago)
            .collect()
    }

    /// Total hours worked this week (completed entries only)
    pub fn worked_hours_this_week(&self) -> f64 {
        self.this_week_history()
            .iter()
            .filter_map(|e| {
                let end = e.end_time?;
                let secs = (end - e.start_time).whole_seconds();
                if secs > 0 {
                    Some(secs as f64 / 3600.0)
                } else {
                    None
                }
            })
            .sum()
    }

    /// Flex time = worked hours - scheduled hours
    pub fn flex_hours_this_week(&self) -> f64 {
        self.worked_hours_this_week() - SCHEDULED_HOURS_PER_WEEK
    }

    /// Weekly hours as a percentage of scheduled hours (0–100, clamped)
    pub fn weekly_hours_percent(&self) -> f64 {
        (self.worked_hours_this_week() / SCHEDULED_HOURS_PER_WEEK * 100.0).clamp(0.0, 100.0)
    }

    /// Per-project/activity breakdown for this week (≥ 1% of total, sorted desc)
    pub fn weekly_project_stats(&self) -> Vec<ProjectStat> {
        use std::collections::HashMap;

        let entries = self.this_week_history();
        let mut map: HashMap<String, f64> = HashMap::new();

        for e in &entries {
            if let Some(end) = e.end_time {
                let secs = (end - e.start_time).whole_seconds();
                if secs > 0 {
                    let project = e
                        .project_name
                        .clone()
                        .unwrap_or_else(|| "Unknown".to_string());
                    let activity = e
                        .activity_name
                        .clone()
                        .unwrap_or_else(|| "Unknown".to_string());
                    let key = format!("{} - {}", project, activity);
                    *map.entry(key).or_insert(0.0) += secs as f64 / 3600.0;
                }
            }
        }

        let total: f64 = map.values().sum();
        if total == 0.0 {
            return Vec::new();
        }

        let mut stats: Vec<ProjectStat> = map
            .into_iter()
            .filter_map(|(label, hours)| {
                let percentage = hours / total * 100.0;
                if percentage >= 1.0 {
                    Some(ProjectStat {
                        label,
                        hours,
                        percentage,
                    })
                } else {
                    None
                }
            })
            .collect();

        stats.sort_by(|a, b| {
            b.hours
                .partial_cmp(&a.hours)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.label.cmp(&b.label)) // stable tiebreaker: label alphabetical
        });
        stats
    }
}

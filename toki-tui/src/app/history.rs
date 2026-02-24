use super::*;

impl App {
    /// Build the history list entries (indices into time_entries)
    pub fn rebuild_history_list(&mut self) {
        let month_ago = (OffsetDateTime::now_utc() - time::Duration::days(30)).date();
        let month_ago_str = format!(
            "{:04}-{:02}-{:02}",
            month_ago.year(),
            month_ago.month() as u8,
            month_ago.day()
        );
        self.history_list_entries = self
            .time_entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.date >= month_ago_str)
            .map(|(idx, _)| idx)
            .collect();
    }

    /// Compute overlapping time entries per day.
    ///
    /// Entries with both `start_time` and `end_time` are checked for actual time-range
    /// intersection. Entries without timestamps are skipped — they can't be checked by
    /// time range and exceeding scheduled hours is legitimate (it just adds flex time).
    pub(super) fn compute_overlaps(&mut self) {
        self.overlapping_entry_ids.clear();

        use std::collections::HashMap;
        let mut entries_by_date: HashMap<&str, Vec<&TimeEntry>> = HashMap::new();

        for entry in &self.time_entries {
            entries_by_date.entry(&entry.date).or_default().push(entry);
        }

        for day_entries in entries_by_date.values() {
            if day_entries.len() < 2 {
                continue;
            }

            let mut intervals: Vec<(&str, i64, i64)> = day_entries
                .iter()
                .filter_map(|entry| {
                    let start = entry.start_time?;
                    let end = entry.end_time?;
                    let start_mins = start.time().hour() as i64 * 60 + start.time().minute() as i64;
                    let end_mins = end.time().hour() as i64 * 60 + end.time().minute() as i64;
                    Some((entry.registration_id.as_str(), start_mins, end_mins))
                })
                .collect();

            intervals.sort_by_key(|(_, start, _)| *start);

            for (i, (_, _, curr_end)) in intervals.iter().enumerate() {
                for (_, next_start, _) in intervals.iter().skip(i + 1) {
                    if *next_start < *curr_end {
                        self.overlapping_entry_ids
                            .insert(intervals[i].0.to_string());
                        if let Some((id, _, _)) = intervals
                            .iter()
                            .skip(i + 1)
                            .find(|(_, s, _)| *s < *curr_end)
                        {
                            self.overlapping_entry_ids.insert(id.to_string());
                        }
                    } else {
                        break;
                    }
                }
            }
        }
    }

    /// Check if an entry has overlapping times
    pub fn is_entry_overlapping(&self, registration_id: &str) -> bool {
        self.overlapping_entry_ids.contains(registration_id)
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
    pub fn this_week_history(&self) -> Vec<&TimeEntry> {
        let now = OffsetDateTime::now_utc();
        let week_start = Self::week_start(now).date();
        let week_end = Self::week_end(now).date();
        let ws = format!(
            "{:04}-{:02}-{:02}",
            week_start.year(),
            week_start.month() as u8,
            week_start.day()
        );
        let we = format!(
            "{:04}-{:02}-{:02}",
            week_end.year(),
            week_end.month() as u8,
            week_end.day()
        );
        self.time_entries
            .iter()
            .filter(|e| e.date >= ws && e.date <= we)
            .collect()
    }

    /// Total hours worked this week (uses Milltime hours directly)
    pub fn worked_hours_this_week(&self) -> f64 {
        self.this_week_history().iter().map(|e| e.hours).sum()
    }

    /// Flex time = worked hours - scheduled hours
    #[allow(dead_code)]
    pub fn flex_hours_this_week(&self) -> f64 {
        self.worked_hours_this_week() - self.scheduled_hours_per_week
    }

    /// Weekly hours as a percentage of scheduled hours (0–100, clamped)
    pub fn weekly_hours_percent(&self) -> f64 {
        (self.worked_hours_this_week() / self.scheduled_hours_per_week * 100.0).clamp(0.0, 100.0)
    }

    /// Per-project/activity breakdown for this week (≥ 1% of total, sorted desc)
    pub fn weekly_project_stats(&self) -> Vec<ProjectStat> {
        use std::collections::HashMap;

        let entries = self.this_week_history();
        let mut map: HashMap<String, f64> = HashMap::new();

        for e in &entries {
            if e.hours > 0.0 {
                let key = format!("{}: {}", e.project_name, e.activity_name);
                *map.entry(key).or_insert(0.0) += e.hours;
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

    /// Per-day breakdown for this week, Mon–Sun, each day split by project/activity.
    /// Projects are colored by their global rank (same order as weekly_project_stats).
    pub fn weekly_daily_stats(&self) -> Vec<DayStat> {
        use std::collections::HashMap;

        // Build the global project ordering (for consistent palette indices)
        let global_stats = self.weekly_project_stats();
        let color_index: HashMap<String, usize> = global_stats
            .iter()
            .enumerate()
            .map(|(i, s)| (s.label.clone(), i))
            .collect();

        // Build 7 slots Mon(0)…Sun(6)
        let day_names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
        let mut slots: Vec<HashMap<String, f64>> = vec![HashMap::new(); 7];

        for entry in self.this_week_history() {
            if entry.hours <= 0.0 {
                continue;
            }
            // Parse entry date to find which weekday slot
            let Some(date) = parse_date_str(&entry.date) else {
                continue;
            };

            let slot = date.weekday().number_days_from_monday() as usize;
            let key = format!("{}: {}", entry.project_name, entry.activity_name);
            *slots[slot].entry(key).or_insert(0.0) += entry.hours;
        }

        slots
            .into_iter()
            .enumerate()
            .map(|(i, map)| {
                let total_hours: f64 = map.values().sum();
                let mut projects: Vec<DailyProjectStat> = map
                    .into_iter()
                    .map(|(label, hours)| {
                        let ci = *color_index.get(&label).unwrap_or(&0);
                        DailyProjectStat {
                            label,
                            hours,
                            color_index: ci,
                        }
                    })
                    .collect();
                // Sort by global rank (color_index) so stacking order matches pie
                projects.sort_by_key(|p| p.color_index);
                DayStat {
                    day_name: day_names[i].to_string(),
                    total_hours,
                    projects,
                }
            })
            .collect()
    }
}

/// Parse a date string in "YYYY-MM-DD" format into a [`time::Date`].
/// Returns `None` if the string is malformed.
pub fn parse_date_str(s: &str) -> Option<time::Date> {
    let mut parts = s.splitn(3, '-');
    let year: i32 = parts.next()?.parse().ok()?;
    let month_u8: u8 = parts.next()?.parse().ok()?;
    let day: u8 = parts.next()?.parse().ok()?;
    let month = time::Month::try_from(month_u8).ok()?;
    time::Date::from_calendar_date(year, month, day).ok()
}

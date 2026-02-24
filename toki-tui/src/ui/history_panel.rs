use super::widgets::{
    build_display_row, build_edit_row, build_running_timer_display_row,
    build_running_timer_edit_row,
};
use super::*;

pub fn render_this_week_history(frame: &mut Frame, area: ratatui::layout::Rect, app: &mut App) {
    let this_week_entries: Vec<crate::types::TimeEntry> =
        app.this_week_history().into_iter().cloned().collect();
    let is_today_focused = app.focused_box == crate::app::FocusedBox::Today;
    let is_timer_running = app.timer_state == crate::app::TimerState::Running;

    // Border style depends on focus
    let border_style = if is_today_focused {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default()
    };

    let title = if is_timer_running {
        format!(
            " This Week ({} entries + running) ",
            this_week_entries.len()
        )
    } else {
        format!(" This Week ({} entries) ", this_week_entries.len())
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(border_style)
        .padding(ratatui::widgets::Padding::horizontal(1));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    // Don't return early on empty if timer is running — we still show the running row
    if this_week_entries.is_empty() && !is_timer_running {
        return;
    }

    let max_rows = inner_area.height as usize;
    app.this_week_view_height = max_rows;

    let today = time::OffsetDateTime::now_utc()
        .to_offset(time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC))
        .date();
    let yesterday = today - time::Duration::days(1);

    // Format today/yesterday as YYYY-MM-DD strings for comparison with entry.date
    let today_str = format!(
        "{:04}-{:02}-{:02}",
        today.year(),
        today.month() as u8,
        today.day()
    );
    let yesterday_str = format!(
        "{:04}-{:02}-{:02}",
        yesterday.year(),
        yesterday.month() as u8,
        yesterday.day()
    );

    // --- Build all logical rows ---
    enum ThisWeekRow<'a> {
        RunningLabel,
        RunningEntry,
        Separator(String),
        Entry {
            entry: &'a crate::types::TimeEntry,
            visible_entry_idx: usize,
        },
    }

    let mut logical_rows: Vec<ThisWeekRow<'_>> = Vec::new();
    let mut last_date: Option<String> = None;
    let mut visible_entry_idx = 0usize;

    if is_timer_running {
        logical_rows.push(ThisWeekRow::RunningLabel);
        logical_rows.push(ThisWeekRow::RunningEntry);
        visible_entry_idx = 1; // DB entries start at visible_entry_idx = 1
    }

    for entry in &this_week_entries {
        let entry_date = &entry.date;
        if last_date.as_deref() != Some(entry_date.as_str()) {
            let label = if entry_date == &today_str {
                "── Today ──".to_string()
            } else if entry_date == &yesterday_str {
                "── Yesterday ──".to_string()
            } else {
                // Parse YYYY-MM-DD to get weekday
                let weekday_label = parse_date_weekday(entry_date);
                format!("── {} ({}) ──", weekday_label, entry_date)
            };
            logical_rows.push(ThisWeekRow::Separator(label));
            last_date = Some(entry_date.clone());
        }
        logical_rows.push(ThisWeekRow::Entry {
            entry,
            visible_entry_idx,
        });
        visible_entry_idx += 1;
    }

    let total_rows = logical_rows.len();

    // --- Find the logical row index of the focused entry ---
    let focused_logical_row: Option<usize> = app.focused_this_week_index.and_then(|fi| {
        logical_rows.iter().position(|r| match r {
            ThisWeekRow::RunningEntry => fi == 0 && is_timer_running,
            ThisWeekRow::Entry {
                visible_entry_idx, ..
            } => *visible_entry_idx == fi,
            _ => false,
        })
    });

    // --- Clamp scroll ---
    if let Some(focused_row) = focused_logical_row {
        if focused_row >= app.this_week_scroll + max_rows {
            app.this_week_scroll = focused_row + 1 - max_rows;
        }
        if focused_row < app.this_week_scroll {
            app.this_week_scroll = focused_row;
        }
    }
    if max_rows < total_rows && app.this_week_scroll > total_rows - max_rows {
        app.this_week_scroll = total_rows - max_rows;
    }
    if total_rows <= max_rows {
        app.this_week_scroll = 0;
    }

    let scroll_offset = app.this_week_scroll;
    let editing_reg_id: Option<&str> = app
        .this_week_edit_state
        .as_ref()
        .map(|e| e.registration_id.as_str());

    // Reserve 1 column on the right for the scrollbar
    let content_width = if total_rows > max_rows {
        inner_area.width.saturating_sub(1)
    } else {
        inner_area.width
    };

    let mut row_y = inner_area.y;
    let mut row_count = 0;

    for (logical_idx, row) in logical_rows.iter().enumerate() {
        if logical_idx < scroll_offset {
            continue;
        }
        if row_count >= max_rows {
            break;
        }

        match row {
            ThisWeekRow::RunningLabel => {
                let sep_rect = Rect::new(inner_area.x, row_y, content_width, 1);
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        "── Running ──",
                        Style::default().fg(Color::Green),
                    ))),
                    sep_rect,
                );
            }
            ThisWeekRow::RunningEntry => {
                let is_focused = is_today_focused && app.focused_this_week_index == Some(0);
                let is_editing = app
                    .this_week_edit_state
                    .as_ref()
                    .map(|s| s.registration_id.is_empty())
                    .unwrap_or(false);
                let line = if is_editing {
                    build_running_timer_edit_row(app.this_week_edit_state.as_ref().unwrap())
                } else {
                    build_running_timer_display_row(app, is_focused, content_width)
                };
                let row_rect = Rect::new(inner_area.x, row_y, content_width, 1);
                frame.render_widget(
                    Paragraph::new(line).style(Style::default().fg(Color::White)),
                    row_rect,
                );
            }
            ThisWeekRow::Separator(label) => {
                let sep_rect = Rect::new(inner_area.x, row_y, content_width, 1);
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        label.as_str(),
                        Style::default().fg(Color::Cyan),
                    ))),
                    sep_rect,
                );
            }
            ThisWeekRow::Entry {
                entry,
                visible_entry_idx,
            } => {
                let is_focused =
                    is_today_focused && app.focused_this_week_index == Some(*visible_entry_idx);
                let is_editing = editing_reg_id == Some(entry.registration_id.as_str());
                let is_overlapping = app.is_entry_overlapping(&entry.registration_id);
                let line = if is_editing {
                    build_edit_row(
                        entry,
                        app.this_week_edit_state.as_ref().unwrap(),
                        is_focused,
                    )
                } else {
                    build_display_row(entry, is_focused, is_overlapping, content_width)
                };
                let row_rect = Rect::new(inner_area.x, row_y, content_width, 1);
                frame.render_widget(
                    Paragraph::new(line).style(Style::default().fg(Color::White)),
                    row_rect,
                );
            }
        }

        row_y += 1;
        row_count += 1;
    }

    // --- Render scrollbar ---
    if total_rows > max_rows {
        let mut scrollbar_state = ScrollbarState::new(total_rows)
            .position(scroll_offset)
            .viewport_content_length(max_rows);
        frame.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .style(Style::default().fg(Color::DarkGray)),
            inner_area,
            &mut scrollbar_state,
        );
    }
}

/// Parse a YYYY-MM-DD string and return the weekday name, or "Unknown" on failure.
fn parse_date_weekday(date_str: &str) -> &'static str {
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

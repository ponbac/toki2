use super::widgets::{
    build_display_row, build_edit_row, build_running_timer_display_row,
    build_running_timer_edit_row,
};
use super::*;

pub fn render_this_week_history(frame: &mut Frame, area: ratatui::layout::Rect, app: &mut App) {
    let this_week_entries: Vec<crate::types::TimerHistoryEntry> =
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

    let today = time::OffsetDateTime::now_utc().date();
    let yesterday = today - time::Duration::days(1);

    // --- Build all logical rows ---
    enum ThisWeekRow<'a> {
        RunningLabel,
        RunningEntry,
        Separator(String),
        Entry {
            entry: &'a crate::types::TimerHistoryEntry,
            visible_entry_idx: usize,
        },
    }

    let mut logical_rows: Vec<ThisWeekRow<'_>> = Vec::new();
    let mut last_date: Option<time::Date> = None;
    let mut visible_entry_idx = 0usize;

    if is_timer_running {
        logical_rows.push(ThisWeekRow::RunningLabel);
        logical_rows.push(ThisWeekRow::RunningEntry);
        visible_entry_idx = 1; // DB entries start at visible_entry_idx = 1
    }

    for entry in &this_week_entries {
        let entry_date = entry.start_time.date();
        if last_date != Some(entry_date) {
            let label = if entry_date == today {
                "── Today ──".to_string()
            } else if entry_date == yesterday {
                "── Yesterday ──".to_string()
            } else {
                let weekday = match entry_date.weekday() {
                    time::Weekday::Monday => "Monday",
                    time::Weekday::Tuesday => "Tuesday",
                    time::Weekday::Wednesday => "Wednesday",
                    time::Weekday::Thursday => "Thursday",
                    time::Weekday::Friday => "Friday",
                    time::Weekday::Saturday => "Saturday",
                    time::Weekday::Sunday => "Sunday",
                };
                format!("── {} ({}) ──", weekday, entry_date)
            };
            logical_rows.push(ThisWeekRow::Separator(label));
            last_date = Some(entry_date);
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
    let editing_entry_id = app.this_week_edit_state.as_ref().map(|e| e.entry_id);

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
                    .map(|s| s.entry_id == -1)
                    .unwrap_or(false);
                let line = if is_editing {
                    build_running_timer_edit_row(app.this_week_edit_state.as_ref().unwrap())
                } else {
                    build_running_timer_display_row(app, is_focused)
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
                let is_editing = Some(entry.id) == editing_entry_id;
                let is_overlapping = app.is_entry_overlapping(entry.id);
                let line = if is_editing {
                    build_edit_row(
                        entry,
                        app.this_week_edit_state.as_ref().unwrap(),
                        is_focused,
                    )
                } else {
                    build_display_row(entry, is_focused, is_overlapping)
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

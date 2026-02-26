use super::widgets::{build_display_row, build_edit_row};
use super::*;

pub fn render_history_view(frame: &mut Frame, app: &mut App, body: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Min(0),    // History list
            Constraint::Length(3), // Controls
        ])
        .split(body);

    let month_ago = (time::OffsetDateTime::now_utc() - time::Duration::days(30)).date();
    let month_ago_str = format!(
        "{:04}-{:02}-{:02}",
        month_ago.year(),
        month_ago.month() as u8,
        month_ago.day()
    );
    let entries: Vec<(usize, &crate::types::TimeEntry)> = app
        .time_entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.date >= month_ago_str)
        .collect();

    if entries.is_empty() {
        let empty_msg = Paragraph::new("No entries in the last 30 days")
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::White))
                    .title(Span::styled(" History ", Style::default().fg(Color::White)))
                    .padding(ratatui::widgets::Padding::horizontal(1)),
            );
        frame.render_widget(empty_msg, chunks[0]);
    } else {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::White))
            .title(Span::styled(
                format!(" History ({} entries) ", entries.len()),
                Style::default().fg(Color::White),
            ))
            .padding(ratatui::widgets::Padding::horizontal(1));

        let inner_area = block.inner(chunks[0]);
        frame.render_widget(block, chunks[0]);

        let max_rows = inner_area.height as usize;
        app.history_view_height = max_rows;

        // --- Build the full ordered list of logical rows (separators + entries) ---
        enum HistoryRow<'a> {
            Separator(String),
            Entry {
                list_idx: Option<usize>,
                entry: &'a crate::types::TimeEntry,
            },
        }

        let today = time::OffsetDateTime::now_utc().date();
        let yesterday = today - time::Duration::days(1);
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
        let mut logical_rows: Vec<HistoryRow<'_>> = Vec::new();
        let mut last_date: Option<String> = None;

        for (history_idx, entry) in &entries {
            if last_date.as_deref() != Some(&entry.date) {
                let label = if entry.date == today_str {
                    "── Today ──".to_string()
                } else if entry.date == yesterday_str {
                    "── Yesterday ──".to_string()
                } else {
                    format!("── {} ──", entry.date)
                };
                logical_rows.push(HistoryRow::Separator(label));
                last_date = Some(entry.date.clone());
            }
            let list_idx = app
                .history_list_entries
                .iter()
                .position(|&idx| idx == *history_idx);
            logical_rows.push(HistoryRow::Entry { list_idx, entry });
        }

        let total_rows = logical_rows.len();

        // --- Find the logical row index of the focused entry ---
        let focused_logical_row: Option<usize> = app.focused_history_index.and_then(|fi| {
            logical_rows.iter().position(|r| {
                if let HistoryRow::Entry { list_idx, .. } = r {
                    *list_idx == Some(fi)
                } else {
                    false
                }
            })
        });

        // --- Clamp scroll so focused row is visible ---
        if let Some(focused_row) = focused_logical_row {
            // Scroll down if focused row is below visible window
            if focused_row >= app.history_scroll + max_rows {
                app.history_scroll = focused_row + 1 - max_rows;
            }
            // Scroll up if focused row is above visible window
            if focused_row < app.history_scroll {
                app.history_scroll = focused_row;
            }
        }
        // Ensure scroll doesn't go past end
        if max_rows < total_rows && app.history_scroll > total_rows - max_rows {
            app.history_scroll = total_rows - max_rows;
        }
        if total_rows <= max_rows {
            app.history_scroll = 0;
        }

        let scroll_offset = app.history_scroll;

        // --- Render visible rows ---
        let editing_reg_id = app
            .history_edit_state
            .as_ref()
            .map(|e| e.registration_id.as_str());
        let mut row_y = inner_area.y;
        let mut row_count = 0;

        // Reserve 1 column on the right for the scrollbar
        let content_width = if total_rows > max_rows {
            inner_area.width.saturating_sub(1)
        } else {
            inner_area.width
        };

        for (logical_idx, row) in logical_rows.iter().enumerate() {
            if logical_idx < scroll_offset {
                continue;
            }
            if row_count >= max_rows {
                break;
            }

            match row {
                HistoryRow::Separator(label) => {
                    let sep_rect = Rect::new(inner_area.x, row_y, content_width, 1);
                    frame.render_widget(
                        Paragraph::new(Line::from(Span::styled(
                            label.as_str(),
                            Style::default().fg(Color::Cyan),
                        ))),
                        sep_rect,
                    );
                }
                HistoryRow::Entry {
                    list_idx, entry, ..
                } => {
                    let is_focused = app.focused_history_index == *list_idx;
                    let is_editing = editing_reg_id == Some(entry.registration_id.as_str());
                    let is_overlapping = app.is_entry_overlapping(&entry.registration_id);

                    let line = if is_editing {
                        build_edit_row(entry, app.history_edit_state.as_ref().unwrap(), is_focused)
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

    // Controls
    let controls_text = if app.history_edit_state.is_some() {
        vec![
            Span::styled("Tab", Style::default().fg(Color::Yellow)),
            Span::raw(": Next field  "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(": Edit field  "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(": Save & exit  "),
            Span::styled("P/A", Style::default().fg(Color::Yellow)),
            Span::raw(": Change Project/Activity"),
        ]
    } else {
        vec![
            Span::styled("↑↓", Style::default().fg(Color::Yellow)),
            Span::raw(": Navigate  "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(": Edit  "),
            Span::styled("H / Esc", Style::default().fg(Color::Yellow)),
            Span::raw(": Back to timer  "),
            Span::styled("Q", Style::default().fg(Color::Yellow)),
            Span::raw(": Quit"),
        ]
    };

    let controls = Paragraph::new(Line::from(controls_text))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .title(Span::styled(
                    " Controls ",
                    Style::default().fg(Color::DarkGray),
                ))
                .padding(ratatui::widgets::Padding::horizontal(1)),
        );

    frame.render_widget(controls, chunks[1]);
}

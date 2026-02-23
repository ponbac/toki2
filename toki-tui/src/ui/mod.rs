use crate::app::{App, EntryEditField, EntryEditState, SaveAction, View};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph},
    Frame,
};
use time::UtcOffset;

fn to_local_time(dt: time::OffsetDateTime) -> time::OffsetDateTime {
    if let Ok(local_offset) = UtcOffset::current_local_offset() {
        dt.to_offset(local_offset)
    } else {
        dt
    }
}

pub fn render(frame: &mut Frame, app: &App) {
    // Split the full screen: global compact stats header on top, body below
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(2), // Global header: compact stats row
            Constraint::Min(0),    // Body: current view
        ])
        .split(frame.area());

    render_compact_stats(frame, root[0], app);

    let body = root[1];
    match app.current_view {
        View::Timer => render_timer_view(frame, app, body),
        View::History => render_history_view(frame, app, body),
        View::SelectProject => render_project_selection(frame, app, body),
        View::SelectActivity => render_activity_selection(frame, app, body),
        View::EditDescription => {
            if app.taskwarrior_overlay.is_some() {
                render_taskwarrior_overlay(frame, app, body);
            } else {
                render_description_editor(frame, app, body);
            }
        }
        View::SaveAction => render_save_action_dialog(frame, app, body),
        View::Statistics => render_statistics_view(frame, app, body),
    }
}

fn render_timer_view(frame: &mut Frame, app: &App, body: Rect) {
    // Timer box height depends on timer size
    let timer_height = match app.timer_size {
        crate::app::TimerSize::Normal => 3,
        crate::app::TimerSize::Large => 11,
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(timer_height), // Timer display (dynamic)
            Constraint::Length(3),            // Project info
            Constraint::Length(3),            // Description
            Constraint::Min(5),               // Today's history
            Constraint::Length(3),            // Status
            Constraint::Length(4),            // Controls (2 rows)
        ])
        .split(body);

    render_timer(frame, chunks[0], app);
    render_project(frame, chunks[1], app);
    render_description(frame, chunks[2], app);
    render_this_week_history(frame, chunks[3], app);
    render_status(frame, chunks[4], app);
    render_controls(frame, chunks[5]);
}

fn render_history_view(frame: &mut Frame, app: &App, body: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Min(0),    // History list
            Constraint::Length(3), // Controls
        ])
        .split(body);

    let month_ago = time::OffsetDateTime::now_utc() - time::Duration::days(30);
    let entries: Vec<(usize, &crate::api::database::TimerHistoryEntry)> = app
        .timer_history
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.start_time >= month_ago)
        .collect();

    if entries.is_empty() {
        let empty_msg = Paragraph::new("No entries in the last 30 days")
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(Span::styled(
                        " History ",
                        Style::default().fg(Color::DarkGray),
                    ))
                    .padding(ratatui::widgets::Padding::horizontal(1)),
            );
        frame.render_widget(empty_msg, chunks[0]);
    } else {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Span::styled(
                format!(" History ({} entries) ", entries.len()),
                Style::default().fg(Color::DarkGray),
            ))
            .padding(ratatui::widgets::Padding::horizontal(1));

        let inner_area = block.inner(chunks[0]);
        frame.render_widget(block, chunks[0]);

        // Build display with date separators
        let mut last_date: Option<time::Date> = None;
        let mut row_y = inner_area.y;
        let max_rows = inner_area.height as usize;
        let mut row_count = 0;

        // Find which entry is being edited
        let editing_entry_id = app.history_edit_state.as_ref().map(|e| e.entry_id);

        for (history_idx, entry) in &entries {
            if row_count >= max_rows {
                break;
            }

            let entry_date = entry.start_time.date();

            // Add date separator if this is a new date
            if last_date != Some(entry_date) {
                if row_count >= max_rows {
                    break;
                }

                let today = time::OffsetDateTime::now_utc().date();
                let yesterday = today - time::Duration::days(1);

                let date_label = if entry_date == today {
                    "── Today ──"
                } else if entry_date == yesterday {
                    "── Yesterday ──"
                } else {
                    Box::leak(format!("── {} ──", entry_date).into_boxed_str())
                };

                let sep_rect = Rect::new(inner_area.x, row_y, inner_area.width, 1);
                let sep = Paragraph::new(Line::from(Span::styled(
                    date_label,
                    Style::default().fg(Color::White).bg(Color::DarkGray),
                )));
                frame.render_widget(sep, sep_rect);

                row_y += 1;
                row_count += 1;
                last_date = Some(entry_date);
            }

            if row_count >= max_rows {
                break;
            }

            // Find the list index for this entry
            let list_idx = app
                .history_list_entries
                .iter()
                .position(|&idx| idx == *history_idx);

            let is_focused = app.focused_history_index == list_idx;
            let is_editing = Some(entry.id) == editing_entry_id;
            let is_overlapping = app.is_entry_overlapping(entry.id);

            let line = if is_editing {
                build_edit_row(entry, app.history_edit_state.as_ref().unwrap(), is_focused)
            } else {
                build_display_row(entry, is_focused, is_overlapping)
            };

            let row_rect = Rect::new(inner_area.x, row_y, inner_area.width, 1);
            let paragraph = Paragraph::new(line).style(Style::default().fg(Color::White));
            frame.render_widget(paragraph, row_rect);

            row_y += 1;
            row_count += 1;
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
            Span::styled("H/Esc", Style::default().fg(Color::Yellow)),
            Span::raw(": Back to Timer  "),
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

fn render_project_selection(frame: &mut Frame, app: &App, body: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // Search input
            Constraint::Min(0),    // Project list
            Constraint::Length(3), // Controls
        ])
        .split(body);

    // Search input box
    let search_text = if app.project_search_input.value.is_empty() {
        if app.selection_list_focused {
            "Type to search...".to_string()
        } else {
            "█".to_string()
        }
    } else if app.selection_list_focused {
        app.project_search_input.value.clone()
    } else {
        let (before, after) = app.project_search_input.split_at_cursor();
        format!("{}█{}", before, after)
    };
    let search_border = if app.selection_list_focused {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };
    let search_box = Paragraph::new(search_text)
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(search_border)
                .title(" Search ")
                .padding(Padding::horizontal(1)),
        );
    frame.render_widget(search_box, chunks[0]);

    // Project list
    let items: Vec<ListItem> = app
        .filtered_projects
        .iter()
        .enumerate()
        .map(|(i, project)| {
            let text = if let Some(code) = &project.code {
                format!("[{}] {}", code, project.name)
            } else {
                project.name.clone()
            };

            let style = if i == app.filtered_project_index {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(text).style(style)
        })
        .collect();

    // Show count: filtered / total
    let title = if app.project_search_input.value.is_empty() {
        format!(" Projects ({}) ", app.projects.len())
    } else {
        format!(
            " Projects ({}/{}) ",
            app.filtered_projects.len(),
            app.projects.len()
        )
    };

    let list_border = if app.selection_list_focused {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(list_border)
                .title(title)
                .padding(Padding::horizontal(1)),
        )
        .style(Style::default());

    frame.render_widget(list, chunks[1]);

    // Controls
    let controls_text = vec![
        Span::styled("Type", Style::default().fg(Color::Yellow)),
        Span::raw(": Filter  "),
        Span::styled("Tab", Style::default().fg(Color::Yellow)),
        Span::raw(": Focus list  "),
        Span::styled("↑↓/j/k", Style::default().fg(Color::Yellow)),
        Span::raw(": Navigate  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(": Select  "),
        Span::styled("Ctrl+X", Style::default().fg(Color::Yellow)),
        Span::raw(": Clear  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(": Cancel"),
    ];

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

    frame.render_widget(controls, chunks[2]);
}

fn render_activity_selection(frame: &mut Frame, app: &App, body: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // Search input
            Constraint::Min(0),    // Activity list
            Constraint::Length(3), // Controls
        ])
        .split(body);

    // Search input box
    let search_text = if app.activity_search_input.value.is_empty() {
        if app.selection_list_focused {
            "Type to search...".to_string()
        } else {
            "█".to_string()
        }
    } else if app.selection_list_focused {
        app.activity_search_input.value.clone()
    } else {
        let (before, after) = app.activity_search_input.split_at_cursor();
        format!("{}█{}", before, after)
    };
    let search_border = if app.selection_list_focused {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };
    let search_box = Paragraph::new(search_text)
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(search_border)
                .title(" Search ")
                .padding(Padding::horizontal(1)),
        );
    frame.render_widget(search_box, chunks[0]);

    // Activity list
    let items: Vec<ListItem> = app
        .filtered_activities
        .iter()
        .enumerate()
        .map(|(i, activity)| {
            let style = if i == app.filtered_activity_index {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(activity.name.clone()).style(style)
        })
        .collect();

    // Show count: filtered / total
    let title = if app.activity_search_input.value.is_empty() {
        format!(" Activities ({}) ", app.activities.len())
    } else {
        format!(
            " Activities ({}/{}) ",
            app.filtered_activities.len(),
            app.activities.len()
        )
    };

    let list_border = if app.selection_list_focused {
        Style::default().fg(Color::White)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(list_border)
                .title(title)
                .padding(Padding::horizontal(1)),
        )
        .style(Style::default());

    frame.render_widget(list, chunks[1]);

    // Controls
    let controls_text = vec![
        Span::styled("Type", Style::default().fg(Color::Yellow)),
        Span::raw(": Filter  "),
        Span::styled("Tab", Style::default().fg(Color::Yellow)),
        Span::raw(": Focus list  "),
        Span::styled("↑↓/j/k", Style::default().fg(Color::Yellow)),
        Span::raw(": Navigate  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(": Select  "),
        Span::styled("Ctrl+X", Style::default().fg(Color::Yellow)),
        Span::raw(": Clear  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(": Cancel"),
    ];

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

    frame.render_widget(controls, chunks[2]);
}

fn render_timer(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    use crate::app::TimerSize;

    let is_running = matches!(app.timer_state, crate::app::TimerState::Running);
    let is_focused = app.focused_box == crate::app::FocusedBox::Timer;

    let border_style = if is_focused {
        Style::default().fg(Color::Magenta)
    } else if is_running {
        Style::default().fg(Color::Green)
    } else {
        Style::default()
    };

    match app.timer_size {
        TimerSize::Normal => {
            // Original normal-sized timer
            let timer_text = match app.timer_state {
                crate::app::TimerState::Running => {
                    let elapsed = app.format_elapsed();
                    format!("{} ⏵ (running)", elapsed)
                }
                crate::app::TimerState::Stopped => "00:00:00 (not running)".to_string(),
            };

            let timer = Paragraph::new(timer_text)
                .style(Style::default().fg(Color::White))
                .alignment(Alignment::Left)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Timer ")
                        .border_style(border_style)
                        .padding(ratatui::widgets::Padding::horizontal(1)),
                );

            frame.render_widget(timer, area);
        }
        TimerSize::Large => {
            // Large ASCII art timer
            let time_str = match app.timer_state {
                crate::app::TimerState::Running => app.format_elapsed(),
                crate::app::TimerState::Stopped => "00:00:00".to_string(),
            };

            let status = match app.timer_state {
                crate::app::TimerState::Running => "⏵ Running",
                crate::app::TimerState::Stopped => "Not running",
            };

            // Add top padding
            let mut lines = vec![Line::from("")];

            // Add large time digits
            lines.extend(render_large_time(&time_str));

            // Add spacing and status
            lines.push(Line::from(""));
            lines.push(Line::from(vec![Span::styled(
                status,
                Style::default().fg(Color::White),
            )]));

            // Add bottom padding
            lines.push(Line::from(""));

            let timer = Paragraph::new(lines).alignment(Alignment::Center).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Timer ")
                    .border_style(border_style)
                    .padding(ratatui::widgets::Padding::horizontal(1)),
            );

            frame.render_widget(timer, area);
        }
    }
}

fn render_project(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let project = app.current_project_name();
    let activity = app.current_activity_name();
    let project_text = format!("{} / {}", project, activity);

    let is_empty = !app.has_project_activity();
    let is_focused = app.focused_box == crate::app::FocusedBox::ProjectActivity;

    let (border_style, text_color) = if is_focused {
        // Magenta border and white text when focused (takes priority)
        (Style::default().fg(Color::Magenta), Color::White)
    } else if !is_empty {
        // Green border when project/activity selected and not focused
        (Style::default().fg(Color::Green), Color::White)
    } else {
        // Default border when empty and not focused
        (Style::default(), Color::White)
    };

    // Title with underlined P
    let title = vec![
        Span::raw(" "),
        Span::styled("P", Style::default().add_modifier(Modifier::UNDERLINED)),
        Span::raw("roject / Activity "),
    ];

    let widget = Paragraph::new(project_text)
        .style(Style::default().fg(text_color))
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Line::from(title))
                .border_style(border_style)
                .padding(ratatui::widgets::Padding::horizontal(1)),
        );

    frame.render_widget(widget, area);
}

fn render_description(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let description = app.current_description();
    let is_empty = description.is_empty();

    let is_focused = app.focused_box == crate::app::FocusedBox::Description;
    let border_style = if is_focused {
        Style::default().fg(Color::Magenta)
    } else if !is_empty {
        // Green border when annotation has content and not focused
        Style::default().fg(Color::Green)
    } else {
        // Default when empty and not focused
        Style::default()
    };

    // Title with underlined A
    let title = vec![
        Span::raw(" "),
        Span::styled("N", Style::default().add_modifier(Modifier::UNDERLINED)),
        Span::raw("ote "),
    ];

    let widget = Paragraph::new(description)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Line::from(title))
                .border_style(border_style)
                .padding(ratatui::widgets::Padding::horizontal(1)),
        );

    frame.render_widget(widget, area);
}

fn render_this_week_history(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let this_week_entries = app.this_week_history();
    let is_today_focused = app.focused_box == crate::app::FocusedBox::Today;
    let edit_state = &app.this_week_edit_state;

    // Border style depends on focus
    let border_style = if is_today_focused {
        Style::default().fg(Color::Magenta)
    } else {
        Style::default()
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .title(format!(" This Week ({} entries) ", this_week_entries.len()))
        .border_style(border_style)
        .padding(ratatui::widgets::Padding::horizontal(1));

    let inner_area = block.inner(area);
    frame.render_widget(block, area);

    if this_week_entries.is_empty() {
        return;
    }

    let max_rows = inner_area.height as usize;
    let mut row_y = inner_area.y;
    let mut row_count = 0;

    // Find which entry is being edited
    let editing_entry_id = edit_state.as_ref().map(|e| e.entry_id);

    // Render with date separators
    let mut last_date: Option<time::Date> = None;
    let mut visible_entry_idx = 0; // Index for focusing (excludes date separators)

    for entry in &this_week_entries {
        if row_count >= max_rows {
            break;
        }

        let entry_date = entry.start_time.date();

        // Add date separator if this is a new date
        if last_date != Some(entry_date) {
            if row_count >= max_rows {
                break;
            }

            let today = time::OffsetDateTime::now_utc().date();
            let yesterday = today - time::Duration::days(1);

            let date_label = if entry_date == today {
                "── Today ──"
            } else if entry_date == yesterday {
                "── Yesterday ──"
            } else {
                // Format as day name and date
                let weekday = match entry_date.weekday() {
                    time::Weekday::Monday => "Monday",
                    time::Weekday::Tuesday => "Tuesday",
                    time::Weekday::Wednesday => "Wednesday",
                    time::Weekday::Thursday => "Thursday",
                    time::Weekday::Friday => "Friday",
                    time::Weekday::Saturday => "Saturday",
                    time::Weekday::Sunday => "Sunday",
                };
                Box::leak(format!("── {} ({}) ──", weekday, entry_date).into_boxed_str())
            };

            let sep_rect = Rect::new(inner_area.x, row_y, inner_area.width, 1);
            let sep = Paragraph::new(Line::from(Span::styled(
                date_label,
                Style::default().fg(Color::White).bg(Color::DarkGray),
            )));
            frame.render_widget(sep, sep_rect);

            row_y += 1;
            row_count += 1;
            last_date = Some(entry_date);
        }

        if row_count >= max_rows {
            break;
        }

        let is_focused = is_today_focused && app.focused_this_week_index == Some(visible_entry_idx);
        let is_editing = Some(entry.id) == editing_entry_id;
        let is_overlapping = app.is_entry_overlapping(entry.id);

        let line = if is_editing {
            build_edit_row(entry, edit_state.as_ref().unwrap(), is_focused)
        } else {
            build_display_row(entry, is_focused, is_overlapping)
        };

        let row_rect = Rect::new(inner_area.x, row_y, inner_area.width, 1);
        let paragraph = Paragraph::new(line).style(Style::default().fg(Color::White));
        frame.render_widget(paragraph, row_rect);

        row_y += 1;
        row_count += 1;
        visible_entry_idx += 1;
    }
}

fn build_display_row(
    entry: &crate::api::database::TimerHistoryEntry,
    is_focused: bool,
    is_overlapping: bool,
) -> Line<'_> {
    // Warning emoji for overlapping entries
    let warning_prefix = if is_overlapping { "⚠ " } else { "" };

    // Base colors - red for overlapping, normal for non-overlapping
    let time_color = if is_overlapping {
        Color::Red
    } else {
        Color::Yellow
    };
    let duration_color = if is_overlapping {
        Color::Red
    } else {
        Color::Magenta
    };
    let project_color = if is_overlapping {
        Color::Red
    } else {
        Color::Cyan
    };
    let note_color = if is_overlapping {
        Color::Red
    } else {
        Color::Gray
    };

    // Calculate duration in [00h:05m] format
    let duration_display = if let Some(end_time) = entry.end_time {
        let duration = end_time - entry.start_time;
        let total_minutes = duration.whole_minutes();
        let hours = total_minutes / 60;
        let minutes = total_minutes % 60;
        format!("[{:02}h:{:02}m]", hours, minutes)
    } else {
        "[Active]".to_string()
    };

    let project = entry.project_name.as_deref().unwrap_or("Unknown");
    let activity = entry.activity_name.as_deref().unwrap_or("Unknown");
    let note = entry.note.as_deref().unwrap_or("");

    // Start time
    let start_time = to_local_time(entry.start_time).time();
    let start_str = format!("{:02}:{:02}", start_time.hour(), start_time.minute());

    // End time
    let end_time_str = if let Some(end_time) = entry.end_time {
        let t = to_local_time(end_time).time();
        format!("{:02}:{:02}", t.hour(), t.minute())
    } else {
        "??:??".to_string()
    };

    // Truncate note if too long
    let max_note_len = 30;
    let note_display = if note.is_empty() {
        "".to_string()
    } else if note.len() > max_note_len {
        format!("{}[...]", &note[..max_note_len])
    } else {
        note.to_string()
    };

    // Build styled line with colors
    let mut spans = vec![];

    // Warning prefix for overlapping entries
    if is_overlapping {
        spans.push(Span::styled(
            warning_prefix,
            Style::default().fg(Color::Red),
        ));
    }

    spans.extend(vec![
        // Time range
        Span::styled(
            format!("{} - {} ", start_str, end_time_str),
            Style::default().fg(time_color),
        ),
        // Duration
        Span::styled(duration_display, Style::default().fg(duration_color)),
        // Pipe separator
        Span::styled(" | ", Style::default().fg(Color::DarkGray)),
        // Project - Activity
        Span::styled(
            format!("{} - {}", project, activity),
            Style::default().fg(project_color),
        ),
    ]);

    // Add annotation if present
    if !note_display.is_empty() {
        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(note_display, Style::default().fg(note_color)));
    }

    // Apply focus styling: white background with black text
    if is_focused {
        let focused_style = Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD);
        return Line::from(vec![Span::styled(
            spans.iter().map(|s| s.content.as_ref()).collect::<String>(),
            focused_style,
        )]);
    }

    Line::from(spans)
}

/// Render a partial or complete time string with a block cursor.
/// - len >= 5 ("HH:MM"): display as-is, no cursor
/// - len < 5: show typed chars + '█' + space padding to fill 5-char slot
fn time_input_display(s: &str) -> String {
    if s.len() >= 5 {
        format!("[{}]", s)
    } else {
        let filled = s.len();
        let spaces = 5 - filled - 1;
        format!("[{}█{}]", s, " ".repeat(spaces))
    }
}

fn build_edit_row<'a>(
    _entry: &'a crate::api::database::TimerHistoryEntry,
    edit_state: &'a EntryEditState,
    _is_focused: bool,
) -> Line<'a> {
    let mut spans = vec![];

    // Start time field
    let start_value = time_input_display(&edit_state.start_time_input);
    let start_style = match edit_state.focused_field {
        EntryEditField::StartTime => Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    spans.push(Span::styled(start_value, start_style));

    // Separator
    spans.push(Span::styled(" - ", Style::default().fg(Color::White)));

    // End time field
    let end_value = time_input_display(&edit_state.end_time_input);
    let end_style = match edit_state.focused_field {
        EntryEditField::EndTime => Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    spans.push(Span::styled(end_value, end_style));

    // Separator
    spans.push(Span::styled(" | ", Style::default().fg(Color::White)));

    // Project field
    let project_value = format!("[{}]", edit_state.project_name.as_deref().unwrap_or("None"));
    let project_style = match edit_state.focused_field {
        EntryEditField::Project => Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    spans.push(Span::styled(project_value, project_style));

    // Separator
    spans.push(Span::styled(" - ", Style::default().fg(Color::White)));

    // Activity field
    let activity_value = format!(
        "[{}]",
        edit_state.activity_name.as_deref().unwrap_or("None")
    );
    let activity_style = match edit_state.focused_field {
        EntryEditField::Activity => Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    spans.push(Span::styled(activity_value, activity_style));

    // Separator
    spans.push(Span::styled(" | ", Style::default().fg(Color::White)));

    // Note field
    let note_style = match edit_state.focused_field {
        EntryEditField::Note => Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    let note_value = if matches!(edit_state.focused_field, EntryEditField::Note) {
        let (before, after) = edit_state.note.split_at_cursor();
        if edit_state.note.value.is_empty() {
            "[█]".to_string()
        } else {
            format!("[{}█{}]", before, after)
        }
    } else {
        format!(
            "[{}]",
            if edit_state.note.value.is_empty() {
                "None"
            } else {
                &edit_state.note.value
            }
        )
    };
    spans.push(Span::styled(note_value, note_style));

    Line::from(spans)
}

fn render_status(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let contextual_status = app.get_contextual_status();
    let status_text = app.status_message.as_deref().unwrap_or(&contextual_status);

    // Determine message type based on content
    let status_lower = status_text.to_lowercase();
    let is_error = status_lower.contains("error")
        || status_lower.contains("warning")
        || status_lower.contains("no active timer")
        || status_lower.contains("cannot save")
        || status_lower.contains("please select")
        || status_lower.contains("cancelled");

    let is_success = status_lower.contains("updated")
        || status_lower.contains("saved")
        || status_lower.contains("success")
        || status_lower.contains("started")
        || status_lower.contains("stopped")
        || status_lower.contains("cleared")
        || status_lower.contains("loaded");

    let (border_style, text_color) = if is_error {
        (Style::default().fg(Color::Red), Color::Red)
    } else if is_success {
        (Style::default().fg(Color::Green), Color::Green)
    } else {
        (Style::default().fg(Color::White), Color::White)
    };

    let status = Paragraph::new(status_text)
        .style(Style::default().fg(text_color))
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Status ")
                .border_style(border_style)
                .padding(ratatui::widgets::Padding::horizontal(1)),
        );

    frame.render_widget(status, area);
}

fn render_controls(frame: &mut Frame, area: ratatui::layout::Rect) {
    let line1 = vec![
        Span::styled("Space", Style::default().fg(Color::Yellow)),
        Span::raw(": Start/Stop  "),
        Span::styled("Ctrl+S", Style::default().fg(Color::Yellow)),
        Span::raw(": Save (options)  "),
        Span::styled("Ctrl+X", Style::default().fg(Color::Yellow)),
        Span::raw(": Clear  "),
        Span::styled("Tab / ↑↓ / j/k", Style::default().fg(Color::Yellow)),
        Span::raw(": Navigate  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(": Edit"),
    ];

    let line2 = vec![
        Span::styled("P", Style::default().fg(Color::Yellow)),
        Span::raw(": Project  "),
        Span::styled("N", Style::default().fg(Color::Yellow)),
        Span::raw(": Note  "),
        Span::styled("H", Style::default().fg(Color::Yellow)),
        Span::raw(": History  "),
        Span::styled("T", Style::default().fg(Color::Yellow)),
        Span::raw(": Toggle timer size  "),
        Span::styled("S", Style::default().fg(Color::Yellow)),
        Span::raw(": Stats  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(": Exit edit  "),
        Span::styled("Q", Style::default().fg(Color::Yellow)),
        Span::raw(": Quit"),
    ];

    let controls = Paragraph::new(vec![Line::from(line1), Line::from(line2)])
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(Line::from(vec![Span::styled(
                    " Controls ",
                    Style::default().fg(Color::DarkGray),
                )]))
                .border_style(Style::default().fg(Color::DarkGray))
                .padding(ratatui::widgets::Padding::horizontal(1)),
        );

    frame.render_widget(controls, area);
}

fn render_description_editor(frame: &mut Frame, app: &App, body: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // 0: Input field or CWD input
            Constraint::Length(5), // 1: Git context panel
            Constraint::Min(0),    // 2: Spacer
            Constraint::Length(3), // 3: Controls
        ])
        .split(body);

    // Input field (note or CWD change)
    if let Some(cwd_input) = &app.cwd_input {
        let completions_hint = if app.cwd_completions.is_empty() {
            String::new()
        } else {
            format!("  [{}]", app.cwd_completions.join("  "))
        };
        let input_text = {
            let (before, after) = cwd_input.split_at_cursor();
            format!("{}█{}{}", before, after, completions_hint)
        };
        let input = Paragraph::new(input_text)
            .style(Style::default().fg(Color::Yellow))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(" Change Directory ")
                    .padding(Padding::horizontal(1)),
            );
        frame.render_widget(input, chunks[0]);
    } else {
        let (before, after) = app.description_input.split_at_cursor();
        let input_text = format!("{}█{}", before, after);
        let input = Paragraph::new(input_text)
            .style(Style::default().fg(Color::White))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Note ")
                    .padding(Padding::horizontal(1)),
            );
        frame.render_widget(input, chunks[0]);
    }

    // Git context panel
    let has_git = app.git_context.branch.is_some();
    let git_color = if has_git {
        Color::White
    } else {
        Color::DarkGray
    };
    let muted = Color::DarkGray;

    let cwd_str = app.git_context.cwd.to_string_lossy().to_string();
    let branch_str = app.git_context.branch.as_deref().unwrap_or("(no git repo)");
    let commit_str = app.git_context.last_commit.as_deref().unwrap_or("(none)");

    let git_lines = vec![
        Line::from(vec![
            Span::styled("Current directory: ", Style::default().fg(muted)),
            Span::styled(cwd_str, Style::default().fg(Color::Cyan)),
        ]),
        Line::from(vec![
            Span::styled("Current branch:    ", Style::default().fg(muted)),
            Span::styled(branch_str, Style::default().fg(git_color)),
        ]),
        Line::from(vec![
            Span::styled("Last commit:       ", Style::default().fg(muted)),
            Span::styled(commit_str, Style::default().fg(git_color)),
        ]),
    ];

    let git_panel = Paragraph::new(git_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Span::styled(" Info ", Style::default().fg(Color::DarkGray)))
            .padding(Padding::horizontal(1)),
    );
    frame.render_widget(git_panel, chunks[1]);

    // Controls (context-sensitive)
    let controls_text: Vec<Span> = if app.cwd_input.is_some() {
        vec![
            Span::styled("Type", Style::default().fg(Color::Yellow)),
            Span::raw(": Path  "),
            Span::styled("Tab", Style::default().fg(Color::Yellow)),
            Span::raw(": Complete  "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(": Confirm  "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(": Cancel"),
        ]
    } else if app.git_mode {
        let git_key_style = if has_git {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        vec![
            Span::styled("1", git_key_style),
            Span::raw(": Copy/paste branch  "),
            Span::styled("2", git_key_style),
            Span::raw(": Parse & paste branch  "),
            Span::styled("3", git_key_style),
            Span::raw(": Copy/paste last commit  "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(": Cancel"),
        ]
    } else {
        let git_key_style = if has_git {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        vec![
            Span::styled("Type", Style::default().fg(Color::Yellow)),
            Span::raw(": Edit  "),
            Span::styled("Ctrl+X", Style::default().fg(Color::Yellow)),
            Span::raw(": Clear  "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(": Confirm  "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(": Cancel  "),
            Span::styled("Ctrl+D", Style::default().fg(Color::Yellow)),
            Span::raw(": Change directory  "),
            Span::styled("Ctrl+G", git_key_style),
            Span::styled(
                ": Git quick commands  ",
                Style::default().fg(if has_git {
                    Color::Reset
                } else {
                    Color::DarkGray
                }),
            ),
            Span::styled("Ctrl+T", Style::default().fg(Color::Yellow)),
            Span::raw(": Taskwarrior"),
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
    frame.render_widget(controls, chunks[3]);
}

fn render_taskwarrior_overlay(frame: &mut Frame, app: &App, body: Rect) {
    // Render description editor in the background
    render_description_editor(frame, app, body);

    let overlay = match &app.taskwarrior_overlay {
        Some(o) => o,
        None => return,
    };

    // 70% width, 20 rows, centered
    let width = (frame.area().width as f32 * 0.70) as u16;
    let height = 20_u16;
    let area = centered_rect(width, height, frame.area());

    frame.render_widget(Clear, area);

    if let Some(err) = &overlay.error {
        let paragraph = Paragraph::new(err.as_str())
            .style(Style::default().fg(Color::Red))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Yellow))
                    .title(Span::styled(
                        " Taskwarrior — error ",
                        Style::default().fg(Color::Yellow),
                    ))
                    .padding(Padding::horizontal(1)),
            );
        frame.render_widget(paragraph, area);
        return;
    }

    let items: Vec<ListItem> = overlay
        .tasks
        .iter()
        .map(|t| {
            ListItem::new(format!("[{}] {}", t.id, t.description))
                .style(Style::default().fg(Color::White))
        })
        .collect();

    let mut list_state = ListState::default();
    list_state.select(overlay.selected);

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(Span::styled(
                    " Taskwarrior Tasks ",
                    Style::default().fg(Color::Yellow),
                ))
                .padding(Padding::horizontal(1)),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

    frame.render_stateful_widget(list, area, &mut list_state);
}

fn render_save_action_dialog(frame: &mut Frame, app: &App, body: Rect) {
    // Render the normal timer view in the background
    render_timer_view(frame, app, body);

    // Calculate centered position for dialog (50 cols x 10 rows)
    let area = centered_rect(50, 10, frame.area());

    // Clear the area for the dialog
    frame.render_widget(Clear, area);

    // Create option list items
    let options = [
        "1. Save & continue (same project)",
        "2. Save & continue (new project)",
        "3. Save & stop",
        "4. Cancel",
    ];

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let action = match i {
                0 => SaveAction::ContinueSameProject,
                1 => SaveAction::ContinueNewProject,
                2 => SaveAction::SaveAndStop,
                3 => SaveAction::Cancel,
                _ => unreachable!(),
            };

            let style = if action == app.selected_save_action {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::White)
            };

            ListItem::new(*text).style(style)
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(" Save Timer ")
            .padding(Padding::horizontal(1)),
    );

    frame.render_widget(list, area);
}

/// Helper function to create a centered rectangle
fn centered_rect(width: u16, height: u16, r: Rect) -> Rect {
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

/// Digit patterns (5x5 grid, 1 = filled, 0 = empty)
const DIGIT_SIZE: usize = 5;

#[rustfmt::skip]
const DIGIT_0: [u8; DIGIT_SIZE * DIGIT_SIZE] = [
    1, 1, 1, 1, 1,
    1, 1, 0, 1, 1,
    1, 1, 0, 1, 1,
    1, 1, 0, 1, 1,
    1, 1, 1, 1, 1,
];

#[rustfmt::skip]
const DIGIT_1: [u8; DIGIT_SIZE * DIGIT_SIZE] = [
    0, 0, 0, 1, 1,
    0, 0, 0, 1, 1,
    0, 0, 0, 1, 1,
    0, 0, 0, 1, 1,
    0, 0, 0, 1, 1,
];

#[rustfmt::skip]
const DIGIT_2: [u8; DIGIT_SIZE * DIGIT_SIZE] = [
    1, 1, 1, 1, 1,
    0, 0, 0, 1, 1,
    1, 1, 1, 1, 1,
    1, 1, 0, 0, 0,
    1, 1, 1, 1, 1,
];

#[rustfmt::skip]
const DIGIT_3: [u8; DIGIT_SIZE * DIGIT_SIZE] = [
    1, 1, 1, 1, 1,
    0, 0, 0, 1, 1,
    1, 1, 1, 1, 1,
    0, 0, 0, 1, 1,
    1, 1, 1, 1, 1,
];

#[rustfmt::skip]
const DIGIT_4: [u8; DIGIT_SIZE * DIGIT_SIZE] = [
    1, 1, 0, 1, 1,
    1, 1, 0, 1, 1,
    1, 1, 1, 1, 1,
    0, 0, 0, 1, 1,
    0, 0, 0, 1, 1,
];

#[rustfmt::skip]
const DIGIT_5: [u8; DIGIT_SIZE * DIGIT_SIZE] = [
    1, 1, 1, 1, 1,
    1, 1, 0, 0, 0,
    1, 1, 1, 1, 1,
    0, 0, 0, 1, 1,
    1, 1, 1, 1, 1,
];

#[rustfmt::skip]
const DIGIT_6: [u8; DIGIT_SIZE * DIGIT_SIZE] = [
    1, 1, 1, 1, 1,
    1, 1, 0, 0, 0,
    1, 1, 1, 1, 1,
    1, 1, 0, 1, 1,
    1, 1, 1, 1, 1,
];

#[rustfmt::skip]
const DIGIT_7: [u8; DIGIT_SIZE * DIGIT_SIZE] = [
    1, 1, 1, 1, 1,
    0, 0, 0, 1, 1,
    0, 0, 0, 1, 1,
    0, 0, 0, 1, 1,
    0, 0, 0, 1, 1,
];

#[rustfmt::skip]
const DIGIT_8: [u8; DIGIT_SIZE * DIGIT_SIZE] = [
    1, 1, 1, 1, 1,
    1, 1, 0, 1, 1,
    1, 1, 1, 1, 1,
    1, 1, 0, 1, 1,
    1, 1, 1, 1, 1,
];

#[rustfmt::skip]
const DIGIT_9: [u8; DIGIT_SIZE * DIGIT_SIZE] = [
    1, 1, 1, 1, 1,
    1, 1, 0, 1, 1,
    1, 1, 1, 1, 1,
    0, 0, 0, 1, 1,
    1, 1, 1, 1, 1,
];

/// Get the pattern for a digit (0-9)
fn get_digit_pattern(digit: char) -> &'static [u8; DIGIT_SIZE * DIGIT_SIZE] {
    match digit {
        '0' => &DIGIT_0,
        '1' => &DIGIT_1,
        '2' => &DIGIT_2,
        '3' => &DIGIT_3,
        '4' => &DIGIT_4,
        '5' => &DIGIT_5,
        '6' => &DIGIT_6,
        '7' => &DIGIT_7,
        '8' => &DIGIT_8,
        '9' => &DIGIT_9,
        _ => &DIGIT_0, // fallback
    }
}

/// Render time string as large block digits
fn render_large_time(time_str: &str) -> Vec<Line<'_>> {
    let symbol = "█";

    // Parse time string (HH:MM:SS) into individual digits and colons
    let chars: Vec<char> = time_str.chars().collect();

    let mut lines = vec![String::new(); DIGIT_SIZE];

    for ch in chars {
        if ch == ':' {
            // Render colon (2 blocks vertically centered, wider spacing)
            lines[0].push_str("   ");
            lines[1].push_str(" ██");
            lines[2].push_str("   ");
            lines[3].push_str(" ██");
            lines[4].push_str("   ");
            lines[0].push_str("  "); // spacing after colon
            lines[1].push_str("  ");
            lines[2].push_str("  ");
            lines[3].push_str("  ");
            lines[4].push_str("  ");
        } else if ch.is_ascii_digit() {
            // Render digit
            let pattern = get_digit_pattern(ch);
            for row in 0..DIGIT_SIZE {
                for col in 0..DIGIT_SIZE {
                    let idx = row * DIGIT_SIZE + col;
                    if pattern[idx] == 1 {
                        lines[row].push_str(symbol);
                    } else {
                        lines[row].push(' ');
                    }
                }
                lines[row].push(' '); // spacing between digits
            }
        }
    }

    // Convert strings to styled Lines
    lines
        .into_iter()
        .map(|line| {
            Line::from(Span::styled(
                line,
                Style::default()
                    .fg(Color::White)
                    .add_modifier(Modifier::BOLD),
            ))
        })
        .collect()
}

fn render_compact_stats(frame: &mut Frame, area: Rect, app: &App) {
    // Split vertically: 1 blank row, 1 content row (no bottom padding)
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // top padding
            Constraint::Length(1), // content
        ])
        .split(area);
    // Add 2-char horizontal padding on each side
    let content_row = rows[1];
    let area = Rect {
        x: content_row.x + 2,
        y: content_row.y,
        width: content_row.width.saturating_sub(4),
        height: content_row.height,
    };

    let worked = app.worked_hours_this_week();
    let flex = app.flex_hours_this_week();
    let percent_f = app.weekly_hours_percent();

    // Format strings
    let percent = percent_f as u16;
    let worked_h = worked.floor() as u64;
    let worked_m = ((worked - worked_h as f64) * 60.0).round() as u64;
    let worked_str = format!("{}h:{:02}m", worked_h, worked_m);

    let remaining_hours = (crate::app::SCHEDULED_HOURS_PER_WEEK - worked).max(0.0);
    let rem_h = remaining_hours.floor() as u64;
    let rem_m = ((remaining_hours - rem_h as f64) * 60.0).round() as u64;

    let muted = Style::default().fg(Color::DarkGray);
    let white = Style::default().fg(Color::White);
    let yellow = Style::default().fg(Color::Yellow);
    let stats_text = Line::from(vec![
        Span::raw("   "),
        Span::styled("This week:", yellow),
        Span::styled(
            format!(
                " {}% ({} / {}h) ",
                percent,
                worked_str,
                crate::app::SCHEDULED_HOURS_PER_WEEK as u32
            ),
            white,
        ),
        Span::styled(" | ", muted),
        Span::styled(" Remaining:", yellow),
        Span::styled(format!(" {}h:{:02}m ", rem_h, rem_m), white),
    ]);
    let stats_width = stats_text.width() as u16;

    // Format flex label
    let flex_abs = flex.abs();
    let flex_h = flex_abs.floor() as u64;
    let flex_m = ((flex_abs - flex_h as f64) * 60.0).round() as u64;
    let flex_sign = if flex >= 0.0 { " +" } else { " -" };
    let flex_str = format!("{}{}h:{:02}m ", flex_sign, flex_h, flex_m);
    let flex_color = if flex >= 0.0 {
        Color::Green
    } else {
        Color::Red
    };

    // Column widths
    const TITLE: &str = " ■ Toki Timer TUI";
    let title_width = TITLE.len() as u16;
    let flex_col_width = 3 + flex_str.len() as u16; // " | " + value

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(title_width),    // App title
            Constraint::Min(10),                // LineGauge (stretches)
            Constraint::Length(stats_width),    // "This week / Remaining" labels
            Constraint::Length(flex_col_width), // " | " + Flex value
        ])
        .split(area);

    // Render title
    frame.render_widget(
        Paragraph::new(Span::styled(TITLE, Style::default().fg(Color::Yellow))),
        cols[0],
    );
    let (gauge_col, stats_col, flex_col) = (cols[1], cols[2], cols[3]);

    // --- LineGauge (no default label) ---
    let ratio = (percent_f / 100.0).clamp(0.0, 1.0);
    let gauge = ratatui::widgets::LineGauge::default()
        .ratio(ratio)
        .label("")
        .filled_symbol(ratatui::symbols::line::THICK_HORIZONTAL)
        .unfilled_symbol("╌")
        .filled_style(Style::default().fg(Color::Cyan))
        .unfilled_style(Style::default().fg(Color::DarkGray));
    frame.render_widget(gauge, gauge_col);

    // --- Stats labels (right of gauge) ---
    frame.render_widget(Paragraph::new(stats_text), stats_col);

    // --- Flex (separator + colored value) ---
    let flex_line = Line::from(vec![
        Span::styled(" | ", muted),
        Span::styled(flex_str, Style::default().fg(flex_color)),
    ]);
    frame.render_widget(Paragraph::new(flex_line), flex_col);
}

fn render_statistics_view(frame: &mut Frame, app: &App, body: Rect) {
    use tui_piechart::{PieChart, PieSlice};

    let outer = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Min(10),   // Pie chart area
            Constraint::Length(3), // Controls
        ])
        .split(body);

    // --- Pie chart ---
    let stats = app.weekly_project_stats();

    if stats.is_empty() {
        let empty = Paragraph::new("No completed entries this week")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::DarkGray))
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(empty, outer[0]);
    } else {
        let palette = [
            Color::Blue,
            Color::Green,
            Color::Yellow,
            Color::Magenta,
            Color::Cyan,
            Color::Red,
            Color::LightBlue,
            Color::LightGreen,
            Color::LightYellow,
            Color::LightMagenta,
            Color::LightCyan,
            Color::LightRed,
        ];

        // Build owned label strings first (PieSlice borrows &str)
        let label_strings: Vec<String> = stats
            .iter()
            .map(|s| {
                let h = s.hours.floor() as u64;
                let m = ((s.hours - h as f64) * 60.0).round() as u64;
                format!("{}: {:02}h:{:02}m", s.label, h, m)
            })
            .collect();

        let slices: Vec<PieSlice> = label_strings
            .iter()
            .enumerate()
            .map(|(i, label)| {
                let color = palette[i % palette.len()];
                PieSlice::new(label.as_str(), stats[i].percentage, color)
            })
            .collect();

        let pie = PieChart::new(slices)
            .show_legend(true)
            .show_percentages(true);
        frame.render_widget(pie, outer[0]);
    }

    // --- Controls ---
    let stats_controls = vec![
        Span::styled("S", Style::default().fg(Color::Yellow)),
        Span::raw("/"),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(": Back to Timer  "),
        Span::styled("Q", Style::default().fg(Color::Yellow)),
        Span::raw(": Quit"),
    ];
    let controls = Paragraph::new(Line::from(stats_controls))
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
    frame.render_widget(controls, outer[1]);
}

use crate::app::{App, EntryEditField, EntryEditState, SaveAction, View};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Padding, Paragraph},
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
    match app.current_view {
        View::Timer => render_timer_view(frame, app),
        View::History => render_history_view(frame, app),
        View::SelectProject => render_project_selection(frame, app),
        View::SelectActivity => render_activity_selection(frame, app),
        View::EditDescription => render_description_editor(frame, app),
        View::SaveAction => render_save_action_dialog(frame, app),
    }
}

fn render_timer_view(frame: &mut Frame, app: &App) {
    // Timer box height depends on timer size
    let timer_height = match app.timer_size {
        crate::app::TimerSize::Normal => 3,
        crate::app::TimerSize::Large => 11, // 1 top padding + 5 ASCII art + 1 spacing + 1 status + 1 bottom padding + 2 borders
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3),            // Header
            Constraint::Length(timer_height), // Timer display (dynamic)
            Constraint::Length(3),            // Project info
            Constraint::Length(3),            // Description
            Constraint::Min(5),               // Today's history
            Constraint::Length(3),            // Status
            Constraint::Length(4),            // Controls (2 rows)
        ])
        .split(frame.size());

    // Header
    render_header(frame, chunks[0]);

    // Timer display
    render_timer(frame, chunks[1], app);

    // Project info
    render_project(frame, chunks[2], app);

    // Description
    render_description(frame, chunks[3], app);

    // Today's history
    render_this_week_history(frame, chunks[4], app);

    // Status message
    render_status(frame, chunks[5], app);

    // Controls
    render_controls(frame, chunks[6]);
}

fn render_history_view(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(0),    // History list
            Constraint::Length(3), // Controls
        ])
        .split(frame.size());

    // Header
    let title = Paragraph::new("Timer History (Last 30 Days)")
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(title, chunks[0]);

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
            .block(Block::default().borders(Borders::ALL).title(" History "));
        frame.render_widget(empty_msg, chunks[1]);
    } else {
        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" History ({} entries) ", entries.len()))
            .padding(ratatui::widgets::Padding::horizontal(1));

        let inner_area = block.inner(chunks[1]);
        frame.render_widget(block, chunks[1]);

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
                let sep = Paragraph::new(date_label)
                    .style(Style::default().fg(Color::White).bg(Color::DarkGray));
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
            Span::styled("H", Style::default().fg(Color::Yellow)),
            Span::raw(": Back to Timer  "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(": Cancel  "),
            Span::styled("Q", Style::default().fg(Color::Yellow)),
            Span::raw(": Quit"),
        ]
    };

    let controls = Paragraph::new(Line::from(controls_text))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("Controls"));

    frame.render_widget(controls, chunks[2]);
}

fn render_project_selection(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // Search input
            Constraint::Min(0),    // Project list
            Constraint::Length(3), // Controls
        ])
        .split(frame.size());

    // Search input box
    let search_text = if app.project_search_input.is_empty() {
        "Type to search...".to_string()
    } else {
        format!("{}_", app.project_search_input)
    };
    let search_box = Paragraph::new(search_text)
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
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
    let title = if app.project_search_input.is_empty() {
        format!(" Projects ({}) ", app.projects.len())
    } else {
        format!(
            " Projects ({}/{}) ",
            app.filtered_projects.len(),
            app.projects.len()
        )
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .padding(Padding::horizontal(1)),
        )
        .style(Style::default());

    frame.render_widget(list, chunks[1]);

    // Controls
    let controls_text = vec![
        Span::styled("Type", Style::default().fg(Color::Yellow)),
        Span::raw(": Filter  "),
        Span::styled("↑↓", Style::default().fg(Color::Yellow)),
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
        .block(Block::default().borders(Borders::ALL).title(" Controls "));

    frame.render_widget(controls, chunks[2]);
}

fn render_activity_selection(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // Search input
            Constraint::Min(0),    // Activity list
            Constraint::Length(3), // Controls
        ])
        .split(frame.size());

    // Search input box
    let search_text = if app.activity_search_input.is_empty() {
        "Type to search...".to_string()
    } else {
        format!("{}_", app.activity_search_input)
    };
    let search_box = Paragraph::new(search_text)
        .style(Style::default().fg(Color::White))
        .alignment(Alignment::Left)
        .block(
            Block::default()
                .borders(Borders::ALL)
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
    let title = if app.activity_search_input.is_empty() {
        format!(" Activities ({}) ", app.activities.len())
    } else {
        format!(
            " Activities ({}/{}) ",
            app.filtered_activities.len(),
            app.activities.len()
        )
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title)
                .padding(Padding::horizontal(1)),
        )
        .style(Style::default());

    frame.render_widget(list, chunks[1]);

    // Controls
    let controls_text = vec![
        Span::styled("Type", Style::default().fg(Color::Yellow)),
        Span::raw(": Filter  "),
        Span::styled("↑↓", Style::default().fg(Color::Yellow)),
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
        .block(Block::default().borders(Borders::ALL).title(" Controls "));

    frame.render_widget(controls, chunks[2]);
}

fn render_header(frame: &mut Frame, area: ratatui::layout::Rect) {
    let title = Paragraph::new("Toki Time Tracking TUI")
        .style(Style::default().fg(Color::Yellow))
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::DarkGray))
                .padding(ratatui::widgets::Padding::horizontal(1)),
        );
    frame.render_widget(title, area);
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
            let sep = Paragraph::new(date_label)
                .style(Style::default().fg(Color::White).bg(Color::DarkGray));
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

fn build_edit_row<'a>(
    _entry: &'a crate::api::database::TimerHistoryEntry,
    edit_state: &'a EntryEditState,
    _is_focused: bool,
) -> Line<'a> {
    let mut spans = vec![];

    // Start time field
    let start_value = if edit_state.start_time_input.len() < 5 {
        format!("[{:>5}]", edit_state.start_time_input)
    } else {
        format!("[{}]", edit_state.start_time_input)
    };
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
    let end_value = if edit_state.end_time_input.len() < 5 {
        format!("[{:>5}]", edit_state.end_time_input)
    } else {
        format!("[{}]", edit_state.end_time_input)
    };
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
    let note_value = format!(
        "[{}]",
        if edit_state.note.is_empty() {
            "None"
        } else {
            &edit_state.note
        }
    );
    let note_style = match edit_state.focused_field {
        EntryEditField::Note => Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
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
        Span::styled(
            "Tab/Shift+Tab / ↑↓ / j/k",
            Style::default().fg(Color::Yellow),
        ),
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

fn render_description_editor(frame: &mut Frame, app: &App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(3), // Input field
            Constraint::Min(0),    // Spacer
            Constraint::Length(3), // Controls
        ])
        .split(frame.size());

    // Header
    let title = Paragraph::new("Edit Note")
        .style(Style::default().fg(Color::Cyan))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(title, chunks[0]);

    // Input field with cursor
    let input_text = format!("{}█", app.description_input); // Add block cursor
    let input = Paragraph::new(input_text)
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Note ")
                .padding(Padding::horizontal(1)),
        );
    frame.render_widget(input, chunks[1]);

    // Controls
    let controls_text = vec![
        Span::styled("Type", Style::default().fg(Color::Yellow)),
        Span::raw(": Edit  "),
        Span::styled("Ctrl+X", Style::default().fg(Color::Yellow)),
        Span::raw(": Clear  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(": Confirm  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(": Cancel"),
    ];

    let controls = Paragraph::new(Line::from(controls_text))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("Controls"));

    frame.render_widget(controls, chunks[3]);
}

fn render_save_action_dialog(frame: &mut Frame, app: &App) {
    // Render the normal timer view in the background
    render_timer_view(frame, app);

    // Calculate centered position for dialog (50 cols x 10 rows)
    let area = centered_rect(50, 10, frame.size());

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

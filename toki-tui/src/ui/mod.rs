use crate::app::{App, SaveAction, View};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Padding, Paragraph},
    Frame,
};

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
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Length(3), // Timer display
            Constraint::Length(3), // Project info
            Constraint::Length(3), // Description
            Constraint::Min(5),    // Today's history
            Constraint::Length(3), // Status
            Constraint::Length(4), // Controls (2 rows)
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
    render_todays_history(frame, chunks[4], app);

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
    let title = Paragraph::new("Timer History")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL));
    frame.render_widget(title, chunks[0]);

    // History list - grouped by date
    let mut items: Vec<ListItem> = Vec::new();
    let mut last_date: Option<time::Date> = None;

    for entry in &app.timer_history {
        let entry_date = entry.start_time.date();

        // Add date separator if this is a new date
        if last_date != Some(entry_date) {
            // Format date as "Today", "Yesterday", or full date
            let today = time::OffsetDateTime::now_utc().date();
            let yesterday = today - time::Duration::days(1);

            let date_label = if entry_date == today {
                "── Today ──".to_string()
            } else if entry_date == yesterday {
                "── Yesterday ──".to_string()
            } else {
                format!("── {} ──", entry_date)
            };

            items.push(ListItem::new(Line::from(vec![Span::styled(
                date_label,
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )])));

            last_date = Some(entry_date);
        }

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

        let project = entry.project_name.as_deref().unwrap_or("No project");
        let activity = entry.activity_name.as_deref().unwrap_or("No activity");
        let note = entry.note.as_deref().unwrap_or("");

        // Start time
        let start_time = entry.start_time.time();
        let start_str = format!("{:02}:{:02}", start_time.hour(), start_time.minute());

        // End time
        let end_time_str = if let Some(end_time) = entry.end_time {
            let t = end_time.time();
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
        let mut spans = vec![
            // Time range in Cyan
            Span::styled(
                format!("{} - {} ", start_str, end_time_str),
                Style::default().fg(Color::Cyan),
            ),
            // Duration in Yellow
            Span::styled(duration_display, Style::default().fg(Color::Yellow)),
            // Pipe separator in Dark Gray
            Span::styled(" | ", Style::default().fg(Color::DarkGray)),
            // Project - Activity in Magenta
            Span::styled(
                format!("{} - {}", project, activity),
                Style::default().fg(Color::Magenta),
            ),
        ];

        // Add annotation if present
        if !note_display.is_empty() {
            spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
            spans.push(Span::styled(note_display, Style::default().fg(Color::Gray)));
        }

        items.push(ListItem::new(Line::from(spans)));
    }

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("History ({} entries)", app.timer_history.len())),
        )
        .style(Style::default().fg(Color::White));

    frame.render_widget(list, chunks[1]);

    // Controls
    let controls_text = vec![
        Span::styled("↑↓", Style::default().fg(Color::Yellow)),
        Span::raw(": Scroll  "),
        Span::styled("H", Style::default().fg(Color::Yellow)),
        Span::raw(": Back to Timer  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(": Cancel  "),
        Span::styled("Q", Style::default().fg(Color::Yellow)),
        Span::raw(": Quit"),
    ];

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
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
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
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
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
    let title = Paragraph::new("Toki Timer TUI")
        .style(
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
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
    let is_running = matches!(app.timer_state, crate::app::TimerState::Running);

    let timer_text = match app.timer_state {
        crate::app::TimerState::Running => {
            let elapsed = app.format_elapsed();
            format!("{} ⏵ (running)", elapsed)
        }
        crate::app::TimerState::Stopped => "00:00:00 (not running)".to_string(),
    };

    let is_focused = app.focused_box == crate::app::FocusedBox::Timer;
    let border_style = if is_focused {
        // Magenta border when focused (takes priority)
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD)
    } else if is_running {
        // Green border when running and not focused
        Style::default().fg(Color::Green)
    } else {
        // Default border when stopped and not focused
        Style::default()
    };

    let timer = Paragraph::new(timer_text)
        .style(
            Style::default()
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        )
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

fn render_project(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let project = app.current_project_name();
    let activity = app.current_activity_name();
    let project_text = format!("{} / {}", project, activity);

    let is_empty = !app.has_project_activity();
    let is_focused = app.focused_box == crate::app::FocusedBox::ProjectActivity;

    let (border_style, text_color) = if is_focused {
        // Magenta border and white text when focused (takes priority)
        (
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            Color::White,
        )
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
        Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD)
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
        Span::styled("A", Style::default().add_modifier(Modifier::UNDERLINED)),
        Span::raw("nnotation "),
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

fn render_todays_history(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let todays_entries = app.todays_history();

    let items: Vec<ListItem> = todays_entries
        .iter()
        .map(|entry| {
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
            let start_time = entry.start_time.time();
            let start_str = format!("{:02}:{:02}", start_time.hour(), start_time.minute());

            // End time
            let end_time_str = if let Some(end_time) = entry.end_time {
                let t = end_time.time();
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
            let mut spans = vec![
                // Time range in Cyan
                Span::styled(
                    format!("{} - {} ", start_str, end_time_str),
                    Style::default().fg(Color::Cyan),
                ),
                // Duration in Yellow
                Span::styled(duration_display, Style::default().fg(Color::Yellow)),
                // Pipe separator in Dark Gray
                Span::styled(" | ", Style::default().fg(Color::DarkGray)),
                // Project - Activity in Magenta
                Span::styled(
                    format!("{} - {}", project, activity),
                    Style::default().fg(Color::Magenta),
                ),
            ];

            // Add annotation if present
            if !note_display.is_empty() {
                spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
                spans.push(Span::styled(note_display, Style::default().fg(Color::Gray)));
            }

            ListItem::new(Line::from(spans))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!(" Today ({} entries) ", todays_entries.len()))
                .padding(ratatui::widgets::Padding::horizontal(1)),
        )
        .style(Style::default().fg(Color::White));

    frame.render_widget(list, area);
}

fn render_status(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
    let contextual_status = app.get_contextual_status();
    let status_text = app.status_message.as_deref().unwrap_or(&contextual_status);

    // Check if status is an error/warning
    let is_error = status_text.to_lowercase().contains("error")
        || status_text.to_lowercase().contains("warning")
        || status_text.to_lowercase().contains("no active timer")
        || status_text.to_lowercase().contains("cannot save")
        || status_text.to_lowercase().contains("please select");

    let border_style = if is_error {
        Style::default().fg(Color::Red)
    } else {
        Style::default()
    };

    let status = Paragraph::new(status_text)
        .style(Style::default().fg(Color::Gray))
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
        Span::raw(": Start  "),
        Span::styled("Ctrl+S", Style::default().fg(Color::Yellow)),
        Span::raw(": Save  "),
        Span::styled(
            "Tab/Shift+Tab / ↑↓ / j/k",
            Style::default().fg(Color::Yellow),
        ),
        Span::raw(": Navigate"),
    ];

    let line2 = vec![
        Span::styled("P", Style::default().fg(Color::Yellow)),
        Span::raw(": Project (add/edit)  "),
        Span::styled("A", Style::default().fg(Color::Yellow)),
        Span::raw(": Annotation (add/edit)  "),
        Span::styled("H", Style::default().fg(Color::Yellow)),
        Span::raw(": History  "),
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
    let title = Paragraph::new("Edit Annotation")
        .style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
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
                .title("Annotation")
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
        "3. Save & pause",
        "4. Cancel",
    ];

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let action = match i {
                0 => SaveAction::ContinueSameProject,
                1 => SaveAction::ContinueNewProject,
                2 => SaveAction::SaveAndPause,
                3 => SaveAction::Cancel,
                _ => unreachable!(),
            };

            let style = if action == app.selected_save_action {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
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

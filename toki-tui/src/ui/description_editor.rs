use super::utils::centered_rect;
use super::*;
use crate::log_notes;

pub fn render_description_editor(frame: &mut Frame, app: &App, body: Rect) {
    let has_log = app.description_log_id.is_some();

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(2)
        .constraints([
            Constraint::Length(3), // 0: Input field or CWD input
            Constraint::Length(6), // 1: Info panel (4 lines: cwd, branch, commit, log path)
            Constraint::Min(3),    // 2: Log content box (empty space when no log)
            Constraint::Min(0),    // 3: Spacer
            Constraint::Length(3), // 4: Controls
        ])
        .split(body);

    // Input field (note or CWD change)
    if let Some(cwd_input) = &app.cwd_input {
        let completions_hint = if app.cwd_completions.is_empty() {
            String::new()
        } else {
            format!("  [{}]", app.cwd_completions.join("  "))
        };
        let (before, after) = cwd_input.split_at_cursor();
        let input_text = format!("{}{}{}", before, after, completions_hint);
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
        // Place terminal cursor: border(1) + padding(1) + char offset
        let cx = chunks[0].x + 2 + before.chars().count() as u16;
        let cy = chunks[0].y + 1;
        frame.set_cursor_position((cx, cy));
    } else {
        // Strip the log tag from the displayed value — the user sees the clean summary.
        // The raw value (including tag) is preserved in app.description_input.value.
        let raw = &app.description_input.value;
        let stripped = log_notes::strip_tag(raw);
        // Compute cursor position in the stripped view (capped at stripped length)
        let cursor = app.description_input.cursor.min(stripped.chars().count());
        let before: String = stripped.chars().take(cursor).collect();
        let after: String = stripped.chars().skip(cursor).collect();
        let input_text = format!("{}{}", before, after);
        let input = Paragraph::new(input_text)
            .style(Style::default().fg(Color::White))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Note ")
                    .padding(Padding::horizontal(1)),
            );
        frame.render_widget(input, chunks[0]);
        // Place terminal cursor: border(1) + padding(1) + char offset
        let cx = chunks[0].x + 2 + cursor as u16;
        let cy = chunks[0].y + 1;
        frame.set_cursor_position((cx, cy));
    }

    // Info panel
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

    // Build log file path label (4th info line)
    let log_path_line = if let Some(ref id) = app.description_log_id {
        match log_notes::log_path(id) {
            Ok(path) => {
                // Show path relative to home if possible
                let home = dirs::home_dir().unwrap_or_default();
                let display = match path.strip_prefix(&home) {
                    Ok(rel) => format!("~/{}", rel.to_string_lossy()),
                    Err(_) => path.to_string_lossy().to_string(),
                };
                Line::from(vec![
                    Span::styled("Log file:          ", Style::default().fg(muted)),
                    Span::styled(display, Style::default().fg(Color::Cyan)),
                ])
            }
            Err(_) => Line::from(vec![Span::styled(
                "Log file:          (error)",
                Style::default().fg(muted),
            )]),
        }
    } else {
        Line::from(vec![Span::styled(
            "Log file:          ",
            Style::default().fg(muted),
        )])
    };

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
        log_path_line,
    ];

    let git_panel = Paragraph::new(git_lines).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(Span::styled(" Info ", Style::default().fg(Color::DarkGray)))
            .padding(Padding::horizontal(1)),
    );
    frame.render_widget(git_panel, chunks[1]);

    // Log content box (read-only, shown when a log is linked)
    if has_log {
        let log_content = app
            .cached_log_content
            .as_deref()
            .unwrap_or_default()
            .to_string();

        let log_paragraph = Paragraph::new(log_content)
            .style(Style::default().fg(Color::DarkGray))
            .wrap(ratatui::widgets::Wrap { trim: false })
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::DarkGray))
                    .title(Span::styled(" Log ", Style::default().fg(Color::DarkGray)))
                    .padding(Padding::horizontal(1)),
            );
        frame.render_widget(log_paragraph, chunks[2]);
    }

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
            Span::styled("B", git_key_style),
            Span::raw(": Copy/paste branch  "),
            Span::styled("P", git_key_style),
            Span::raw(": Parse & paste branch  "),
            Span::styled("C", git_key_style),
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
        let mut spans = vec![
            Span::styled("Type", Style::default().fg(Color::Yellow)),
            Span::raw(": Edit  "),
            Span::styled("Ctrl+X", Style::default().fg(Color::Yellow)),
            Span::raw(": Clear  "),
            Span::styled("Enter", Style::default().fg(Color::Yellow)),
            Span::raw(": Confirm  "),
            Span::styled("Esc", Style::default().fg(Color::Yellow)),
            Span::raw(": Cancel  "),
            Span::styled("Ctrl+L", Style::default().fg(Color::Yellow)),
            Span::raw(": Add/edit log file  "),
        ];
        if has_log {
            spans.push(Span::styled("Ctrl+R", Style::default().fg(Color::Yellow)));
            spans.push(Span::raw(": Remove log file  "));
        }
        spans.extend([
            Span::styled("Ctrl+D", Style::default().fg(Color::Yellow)),
            Span::raw(": Change directory  "),
            Span::styled("Ctrl+G", git_key_style),
            Span::styled(
                ": Git  ",
                Style::default().fg(if has_git {
                    Color::Reset
                } else {
                    Color::DarkGray
                }),
            ),
            Span::styled("Ctrl+T", Style::default().fg(Color::Yellow)),
            Span::raw(": Taskwarrior"),
        ]);
        spans
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
    frame.render_widget(controls, chunks[4]);
}

pub fn render_taskwarrior_overlay(frame: &mut Frame, app: &App, body: Rect) {
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

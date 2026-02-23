use super::utils::centered_rect;
use super::*;

pub fn render_description_editor(frame: &mut Frame, app: &App, body: Rect) {
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

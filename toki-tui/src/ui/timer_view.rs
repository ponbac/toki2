use super::*;

pub fn render_timer_view(frame: &mut Frame, app: &mut App, body: Rect) {
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
    super::history_panel::render_this_week_history(frame, chunks[3], app);
    render_status(frame, chunks[4], app);
    render_controls(frame, chunks[5]);
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
    let project_text = format!("{}: {}", project, activity);

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

pub fn render_status(frame: &mut Frame, area: ratatui::layout::Rect, app: &App) {
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
        Span::styled("S", Style::default().fg(Color::Yellow)),
        Span::raw(": Statistics  "),
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
            #[allow(clippy::needless_range_loop)]
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

pub fn render_compact_stats(frame: &mut Frame, area: Rect, app: &mut App) {
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
    // Use Milltime's total accumulated flex, adjusted live for the running timer
    let timer_elapsed_hours = app.elapsed_duration().as_secs_f64() / 3600.0;
    let flex = app.flex_time_current + timer_elapsed_hours;
    let percent_f = app.weekly_hours_percent();

    // Format strings
    let percent = percent_f.round() as u16;
    let worked_h = worked.floor() as u64;
    let worked_m = ((worked - worked_h as f64) * 60.0).round() as u64;
    let worked_str = format!("{}h:{:02}m", worked_h, worked_m);

    let remaining_hours = (app.scheduled_hours_per_week - worked).max(0.0);
    let rem_h = remaining_hours.floor() as u64;
    let rem_m = ((remaining_hours - rem_h as f64) * 60.0).round() as u64;

    let muted = Style::default().fg(Color::DarkGray);
    let white = Style::default().fg(Color::White);
    let yellow = Style::default().fg(Color::Yellow);
    let stats_text = Line::from(vec![
        Span::raw("   "),
        Span::styled("This week:", yellow),
        Span::styled(format!(" {}%", percent), white),
        Span::styled(
            format!(" ({} / {}h) ", worked_str, app.scheduled_hours_per_week),
            muted,
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

    // Column widths — throbber (1 char) + " Toki Timer TUI"
    const LABEL: &str = " Toki Timer TUI";
    let title_width = 1 + LABEL.len() as u16 + 1; // leading space + symbol + label
    let flex_col_width = 3 + "Flex:".len() as u16 + flex_str.len() as u16; // " | " + "Flex:" + value

    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Length(title_width),    // App title
            Constraint::Min(10),                // LineGauge (stretches)
            Constraint::Length(stats_width),    // "This week / Remaining" labels
            Constraint::Length(flex_col_width), // " | " + Flex value
        ])
        .split(area);

    // Render title: throbber (spinning when loading, full symbol when idle) + label
    let throbber_area = Rect {
        x: cols[0].x + 1,
        y: cols[0].y,
        width: 1,
        height: 1,
    };
    let label_area = Rect {
        x: throbber_area.x + 1,
        y: cols[0].y,
        width: cols[0].width.saturating_sub(2),
        height: 1,
    };
    let throbber = throbber_widgets_tui::Throbber::default()
        .style(Style::default().fg(Color::Yellow))
        .throbber_style(Style::default().fg(Color::Yellow))
        .throbber_set(throbber_widgets_tui::BRAILLE_SIX)
        .use_type(if app.is_loading {
            throbber_widgets_tui::WhichUse::Spin
        } else {
            throbber_widgets_tui::WhichUse::Full
        });
    frame.render_stateful_widget(throbber, throbber_area, &mut app.throbber_state);
    frame.render_widget(
        Paragraph::new(Span::styled(LABEL, Style::default().fg(Color::Yellow))),
        label_area,
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
    let flex_color = if flex >= 0.0 {
        Style::default().fg(Color::Green)
    } else {
        Style::default().fg(Color::Red)
    };
    let flex_line = Line::from(vec![
        Span::styled(" | ", muted),
        Span::styled("Flex:", yellow),
        Span::styled(flex_str, flex_color),
    ]);
    frame.render_widget(Paragraph::new(flex_line), flex_col);
}

use super::*;

/// Height of the large digit block (5 rows of pixels)
const CLOCK_ROWS: u16 = 5;

pub fn render_zen_view(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let muted = Style::default().fg(Color::DarkGray);

    let is_running = matches!(app.timer_state, crate::app::TimerState::Running);

    // Content block: clock + optional project line
    // 5 clock rows + 1 blank + (1 if running, else 0)
    let info_rows: u16 = if is_running { 1 } else { 0 };
    let content_height = CLOCK_ROWS + 1 + info_rows;

    // Split frame: [top padding] [content] [bottom padding] [1 hint row]
    let hint_height: u16 = 1;
    let remaining = area.height.saturating_sub(content_height + hint_height);
    let top_pad = remaining / 2;
    let bot_pad = remaining - top_pad;

    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(top_pad),
            Constraint::Length(content_height),
            Constraint::Length(bot_pad),
            Constraint::Length(hint_height),
        ])
        .split(area);

    // --- Clock ---
    let time_str = match app.timer_state {
        crate::app::TimerState::Running => app.format_elapsed(),
        crate::app::TimerState::Stopped => "00:00:00".to_string(),
    };

    let mut clock_lines = super::timer_view::render_large_time_muted(&time_str);

    // Blank separator
    clock_lines.push(Line::from(""));

    // Project line (only when running)
    if is_running {
        let project = app.current_project_name();
        let activity = app.current_activity_name();
        let proj_line = if app.has_project_activity() {
            format!("{}: {}", project, activity)
        } else {
            String::new()
        };
        clock_lines.push(Line::from(Span::styled(proj_line, muted)));
    }

    let clock_para = Paragraph::new(clock_lines).alignment(Alignment::Center);
    frame.render_widget(clock_para, rows[1]);

    // --- Hint ---
    let hint = Paragraph::new(Line::from(Span::styled("Z / Esc:  Exit zen mode", muted)))
        .alignment(Alignment::Center);
    frame.render_widget(hint, rows[3]);
}

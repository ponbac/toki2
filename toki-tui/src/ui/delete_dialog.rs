use super::utils::centered_rect;
use super::*;
use crate::app::DeleteOrigin;

pub fn render_delete_confirm_dialog(frame: &mut Frame, app: &mut App, body: Rect) {
    // Extract owned values before borrowing `app` mutably for background render
    let (origin, label, detail) = if let Some(ctx) = &app.delete_context {
        let h = format!("{:.2}h", ctx.display_hours);
        let detail = format!("{}  ·  {}", ctx.display_date, h);
        (Some(ctx.origin), ctx.display_label.clone(), detail)
    } else {
        (None, String::new(), String::new())
    };

    // Render the originating view in the background
    if let Some(origin) = origin {
        match origin {
            DeleteOrigin::Timer => super::timer_view::render_timer_view(frame, app, body),
            DeleteOrigin::History => super::history_view::render_history_view(frame, app, body),
        }
    }

    let area = centered_rect(52, 10, frame.area());
    frame.render_widget(Clear, area);

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(label, Style::default().fg(Color::White))),
        Line::from(Span::styled(detail, Style::default().fg(Color::DarkGray))),
        Line::from(""),
        Line::from(vec![
            Span::styled("[y] Yes", Style::default().fg(Color::Red)),
            Span::raw("    "),
            Span::styled("[n] No", Style::default().fg(Color::White)),
        ]),
    ];

    let paragraph = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Delete Entry? ")
                .padding(Padding::horizontal(1)),
        )
        .alignment(Alignment::Center);

    frame.render_widget(paragraph, area);
}

use super::utils::centered_rect;
use super::*;

pub fn render_save_action_dialog(frame: &mut Frame, app: &mut App, body: Rect) {
    // Render the normal timer view in the background
    super::timer_view::render_timer_view(frame, app, body);

    // Calculate centered position for dialog (50 cols x 10 rows)
    let area = centered_rect(50, 10, frame.area());

    // Clear the area for the dialog
    frame.render_widget(Clear, area);

    // Create option list items
    let options = [
        "1. Save & stop",
        "2. Save & continue (new project)",
        "3. Save & continue (same project)",
        "4. Cancel",
    ];

    let items: Vec<ListItem> = options
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let action = match i {
                0 => SaveAction::SaveAndStop,
                1 => SaveAction::ContinueNewProject,
                2 => SaveAction::ContinueSameProject,
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

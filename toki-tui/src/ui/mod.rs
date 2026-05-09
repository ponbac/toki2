use crate::app::{App, SaveAction, View};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{
        Block, Borders, Clear, List, ListItem, ListState, Padding, Paragraph, Scrollbar,
        ScrollbarOrientation, ScrollbarState,
    },
    Frame,
};

mod delete_dialog;
mod description_editor;
mod history_panel;
mod history_view;
mod save_dialog;
mod selection_views;
mod statistics_view;
mod template_selection_view;
mod timer_view;
pub(super) mod utils;
pub(super) mod widgets;
mod zen_view;

pub fn render(frame: &mut Frame, app: &mut App) {
    // Zen mode: full-screen, no stats bar, no other UI
    if app.current_view == View::Timer && app.zen_mode {
        zen_view::render_zen_view(frame, app);
        return;
    }

    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(frame.area());

    timer_view::render_compact_stats(frame, root[0], app);

    let body = root[1];
    match app.current_view {
        View::Timer => timer_view::render_timer_view(frame, app, body),
        View::History => history_view::render_history_view(frame, app, body),
        View::SelectProject => selection_views::render_project_selection(frame, app, body),
        View::SelectActivity => selection_views::render_activity_selection(frame, app, body),
        View::SelectTemplate => {
            template_selection_view::render_template_selection(frame, app, body)
        }
        View::EditDescription => {
            if app.taskwarrior_overlay.is_some() {
                description_editor::render_taskwarrior_overlay(frame, app, body);
            } else {
                description_editor::render_description_editor(frame, app, body);
            }
        }
        View::SaveAction => save_dialog::render_save_action_dialog(frame, app, body),
        View::Statistics => statistics_view::render_statistics_view(frame, app, body),
        View::ConfirmDelete => delete_dialog::render_delete_confirm_dialog(frame, app, body),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{FocusedBox, TimerState};
    use crate::test_support::{activity, project, test_app};
    use ratatui::{backend::TestBackend, Terminal};
    use time::macros::datetime;

    fn render_lines(app: &mut App) -> Vec<String> {
        let backend = TestBackend::new(100, 30);
        let mut terminal = Terminal::new(backend).expect("test terminal");
        terminal
            .draw(|frame| render(frame, app))
            .expect("render should succeed");

        let backend = terminal.backend();
        let buffer = backend.buffer();

        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect()
    }

    fn rendered_text(app: &mut App) -> String {
        render_lines(app).join("\n")
    }

    #[test]
    fn render_shows_running_timer_project_and_note() {
        let mut app = test_app();
        app.timer_state = TimerState::Running;
        app.absolute_start = Some(datetime!(2026-03-06 09:15 UTC));
        app.selected_project = Some(project("proj-1", "Project One"));
        app.selected_activity = Some(activity("act-1", "proj-1", "Activity One"));
        app.description_input.value = "Investigate tests".to_string();
        app.description_input.cursor = app.description_input.value.len();
        app.focused_box = FocusedBox::ProjectActivity;

        let text = rendered_text(&mut app);

        assert!(text.contains("Timer"));
        assert!(text.contains("(running)"));
        assert!(text.contains("Project One: Activity One"));
        assert!(text.contains("Investigate tests"));
    }

    #[test]
    fn render_status_shows_error_copy() {
        let mut app = test_app();
        app.status_message = Some("Error starting timer: boom".to_string());

        let text = rendered_text(&mut app);

        assert!(text.contains("Status"));
        assert!(text.contains("Error starting timer: boom"));
    }

    #[test]
    fn render_status_shows_success_copy() {
        let mut app = test_app();
        app.status_message = Some("Saved 00:15:00 to Project / Activity".to_string());

        let text = rendered_text(&mut app);

        assert!(text.contains("Saved 00:15:00 to Project / Activity"));
    }
}

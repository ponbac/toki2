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
mod timer_view;
pub(super) mod utils;
pub(super) mod widgets;

pub fn render(frame: &mut Frame, app: &mut App) {
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

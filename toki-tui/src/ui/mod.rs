use crate::app::{App, MilltimeReauthField, SaveAction, View};
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
mod zen_view;

pub fn render(frame: &mut Frame, app: &mut App) {
    // Zen mode: full-screen, no stats bar, no other UI
    if app.current_view == View::Timer && app.zen_mode {
        zen_view::render_zen_view(frame, app);
        // Still render milltime reauth on top if needed
        if app.milltime_reauth.is_some() {
            render_milltime_reauth_overlay(frame, app);
        }
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

    // Milltime re-auth overlay — renders on top of any view
    if app.milltime_reauth.is_some() {
        render_milltime_reauth_overlay(frame, app);
    }
}

fn render_milltime_reauth_overlay(frame: &mut Frame, app: &App) {
    let state = match &app.milltime_reauth {
        Some(s) => s,
        None => return,
    };

    let area = utils::centered_rect(60, 14, frame.area());
    frame.render_widget(Clear, area);

    let username_focused = state.focused_field == MilltimeReauthField::Username;
    let password_focused = state.focused_field == MilltimeReauthField::Password;

    let username_style = if username_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let password_style = if password_focused {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default().fg(Color::DarkGray)
    };

    // Password is masked
    let password_display = "•".repeat(state.password_input.value.len());

    let mut lines = vec![
        Line::from(""),
        Line::from(Span::styled(
            "Milltime session expired. Please re-authenticate.",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Username: ", username_style),
            Span::styled(
                state.username_input.value.clone(),
                if username_focused {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                },
            ),
        ]),
        Line::from(vec![
            Span::styled("Password: ", password_style),
            Span::styled(
                password_display,
                if password_focused {
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                },
            ),
        ]),
        Line::from(""),
    ];

    if let Some(err) = &state.error {
        lines.push(Line::from(Span::styled(
            err.as_str(),
            Style::default().fg(Color::Red),
        )));
        lines.push(Line::from(""));
    }

    lines.push(Line::from(vec![
        Span::styled("Tab", Style::default().fg(Color::Yellow)),
        Span::raw(": Switch field  "),
        Span::styled("Enter", Style::default().fg(Color::Yellow)),
        Span::raw(": Submit  "),
        Span::styled("Esc", Style::default().fg(Color::Yellow)),
        Span::raw(": Cancel"),
    ]));

    let paragraph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(Span::styled(
                    " Milltime Re-authentication ",
                    Style::default().fg(Color::Yellow),
                ))
                .padding(Padding::horizontal(2)),
        )
        .alignment(Alignment::Left);

    frame.render_widget(paragraph, area);
}

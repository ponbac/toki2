use super::utils::to_local_time;
use crate::app::{EntryEditField, EntryEditState};
use crate::types::TimerHistoryEntry;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::app::App;

/// Render a partial or complete time string with a block cursor.
/// - len >= 5 ("HH:MM"): display as-is, no cursor
/// - len < 5: show typed chars + '█' + space padding to fill 5-char slot
fn time_input_display(s: &str) -> String {
    if s.len() >= 5 {
        format!("[{}]", s)
    } else {
        let filled = s.len();
        let spaces = 5 - filled - 1;
        format!("[{}█{}]", s, " ".repeat(spaces))
    }
}

pub fn build_display_row(
    entry: &TimerHistoryEntry,
    is_focused: bool,
    is_overlapping: bool,
) -> Line<'_> {
    // Warning emoji for overlapping entries
    let warning_prefix = if is_overlapping { "⚠ " } else { "" };

    // Base colors - red for overlapping, normal for non-overlapping
    let time_color = if is_overlapping {
        Color::Red
    } else {
        Color::Yellow
    };
    let duration_color = if is_overlapping {
        Color::Red
    } else {
        Color::Magenta
    };
    let project_color = if is_overlapping {
        Color::Red
    } else {
        Color::Cyan
    };
    let note_color = if is_overlapping {
        Color::Red
    } else {
        Color::Gray
    };

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
    let start_time = to_local_time(entry.start_time).time();
    let start_str = format!("{:02}:{:02}", start_time.hour(), start_time.minute());

    // End time
    let end_time_str = if let Some(end_time) = entry.end_time {
        let t = to_local_time(end_time).time();
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
    let mut spans = vec![];

    // Warning prefix for overlapping entries
    if is_overlapping {
        spans.push(Span::styled(
            warning_prefix,
            Style::default().fg(Color::Red),
        ));
    }

    spans.extend(vec![
        // Time range
        Span::styled(
            format!("{} - {} ", start_str, end_time_str),
            Style::default().fg(time_color),
        ),
        // Duration
        Span::styled(duration_display, Style::default().fg(duration_color)),
        // Pipe separator
        Span::styled(" | ", Style::default().fg(Color::DarkGray)),
        // Project - Activity
        Span::styled(
            format!("{} - {}", project, activity),
            Style::default().fg(project_color),
        ),
    ]);

    // Add annotation if present
    if !note_display.is_empty() {
        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(note_display, Style::default().fg(note_color)));
    }

    // Apply focus styling: white background with black text
    if is_focused {
        let focused_style = Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD);
        return Line::from(vec![Span::styled(
            spans.iter().map(|s| s.content.as_ref()).collect::<String>(),
            focused_style,
        )]);
    }

    Line::from(spans)
}

pub fn build_running_timer_display_row(app: &App, is_focused: bool) -> Line<'static> {
    let start_str = app
        .absolute_start
        .map(|t| {
            let local = to_local_time(t);
            format!("{:02}:{:02}", local.hour(), local.minute())
        })
        .unwrap_or_else(|| "??:??".to_string());

    let elapsed = app
        .absolute_start
        .map(|start| {
            let now = time::OffsetDateTime::now_utc();
            let diff = now - start;
            std::time::Duration::from_secs(diff.whole_seconds().max(0) as u64)
        })
        .unwrap_or_else(|| app.elapsed_duration());
    let total_mins = elapsed.as_secs() / 60;
    let hours = total_mins / 60;
    let mins = total_mins % 60;
    let duration_str = format!("[{:02}h:{:02}m]", hours, mins);

    let project = app
        .selected_project
        .as_ref()
        .map(|p| p.name.clone())
        .unwrap_or_else(|| "[None]".to_string());
    let activity = app
        .selected_activity
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| "[None]".to_string());
    let note = app.description_input.value.clone();

    let max_note_len = 30;
    let note_display = if note.is_empty() {
        String::new()
    } else if note.len() > max_note_len {
        format!("{}[...]", &note[..max_note_len])
    } else {
        note.clone()
    };

    let text = if note_display.is_empty() {
        format!(
            "▶ {} - HH:MM {} | {} - {}",
            start_str, duration_str, project, activity
        )
    } else {
        format!(
            "▶ {} - HH:MM {} | {} - {} | {}",
            start_str, duration_str, project, activity, note_display
        )
    };

    if is_focused {
        return Line::from(Span::styled(
            text,
            Style::default()
                .fg(Color::Black)
                .bg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
    }

    // Non-focused: color each part
    let mut spans: Vec<Span<'static>> = vec![
        Span::styled("▶ ", Style::default().fg(Color::Green)),
        Span::styled(
            format!("{} - HH:MM ", start_str),
            Style::default().fg(Color::Yellow),
        ),
        Span::styled(duration_str, Style::default().fg(Color::Magenta)),
        Span::styled(" | ", Style::default().fg(Color::DarkGray)),
        Span::styled(
            format!("{} - {}", project, activity),
            Style::default().fg(Color::Cyan),
        ),
    ];
    if !note_display.is_empty() {
        spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
        spans.push(Span::styled(note_display, Style::default().fg(Color::Gray)));
    }
    Line::from(spans)
}

pub fn build_running_timer_edit_row(edit_state: &EntryEditState) -> Line<'_> {
    let mut spans = vec![];

    // ▶ prefix before start time (no space)
    spans.push(Span::styled("▶ ", Style::default().fg(Color::Green)));

    // Start time field
    let start_value = time_input_display(&edit_state.start_time_input);
    let start_style = match edit_state.focused_field {
        EntryEditField::StartTime => Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    spans.push(Span::styled(start_value, start_style));

    // Separator + HH:MM placeholder for end time
    spans.push(Span::styled(
        " - HH:MM | ",
        Style::default().fg(Color::White),
    ));

    // Project field
    let project_value = format!("[{}]", edit_state.project_name.as_deref().unwrap_or("None"));
    let project_style = match edit_state.focused_field {
        EntryEditField::Project => Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    spans.push(Span::styled(project_value, project_style));

    spans.push(Span::styled(" - ", Style::default().fg(Color::White)));

    // Activity field
    let activity_value = format!(
        "[{}]",
        edit_state.activity_name.as_deref().unwrap_or("None")
    );
    let activity_style = match edit_state.focused_field {
        EntryEditField::Activity => Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    spans.push(Span::styled(activity_value, activity_style));

    spans.push(Span::styled(" | ", Style::default().fg(Color::White)));

    // Note field
    let note_style = match edit_state.focused_field {
        EntryEditField::Note => Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    let note_value = if matches!(edit_state.focused_field, EntryEditField::Note) {
        let (before, after) = edit_state.note.split_at_cursor();
        if edit_state.note.value.is_empty() {
            "[█]".to_string()
        } else {
            format!("[{}█{}]", before, after)
        }
    } else {
        format!(
            "[{}]",
            if edit_state.note.value.is_empty() {
                "Empty"
            } else {
                &edit_state.note.value
            }
        )
    };
    spans.push(Span::styled(note_value, note_style));

    Line::from(spans)
}

pub fn build_edit_row<'a>(
    _entry: &'a TimerHistoryEntry,
    edit_state: &'a EntryEditState,
    _is_focused: bool,
) -> Line<'a> {
    let mut spans = vec![];

    // Start time field
    let start_value = time_input_display(&edit_state.start_time_input);
    let start_style = match edit_state.focused_field {
        EntryEditField::StartTime => Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    spans.push(Span::styled(start_value, start_style));

    // Separator
    spans.push(Span::styled(" - ", Style::default().fg(Color::White)));

    // End time field
    let end_value = time_input_display(&edit_state.end_time_input);
    let end_style = match edit_state.focused_field {
        EntryEditField::EndTime => Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    spans.push(Span::styled(end_value, end_style));

    // Separator
    spans.push(Span::styled(" | ", Style::default().fg(Color::White)));

    // Project field
    let project_value = format!("[{}]", edit_state.project_name.as_deref().unwrap_or("None"));
    let project_style = match edit_state.focused_field {
        EntryEditField::Project => Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    spans.push(Span::styled(project_value, project_style));

    // Separator
    spans.push(Span::styled(" - ", Style::default().fg(Color::White)));

    // Activity field
    let activity_value = format!(
        "[{}]",
        edit_state.activity_name.as_deref().unwrap_or("None")
    );
    let activity_style = match edit_state.focused_field {
        EntryEditField::Activity => Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    spans.push(Span::styled(activity_value, activity_style));

    // Separator
    spans.push(Span::styled(" | ", Style::default().fg(Color::White)));

    // Note field
    let note_style = match edit_state.focused_field {
        EntryEditField::Note => Style::default()
            .fg(Color::Black)
            .bg(Color::White)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::White),
    };
    let note_value = if matches!(edit_state.focused_field, EntryEditField::Note) {
        let (before, after) = edit_state.note.split_at_cursor();
        if edit_state.note.value.is_empty() {
            "[█]".to_string()
        } else {
            format!("[{}█{}]", before, after)
        }
    } else {
        format!(
            "[{}]",
            if edit_state.note.value.is_empty() {
                "Empty"
            } else {
                &edit_state.note.value
            }
        )
    };
    spans.push(Span::styled(note_value, note_style));

    Line::from(spans)
}

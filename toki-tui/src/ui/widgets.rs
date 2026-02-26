use super::utils::to_local_time;
use crate::app::{EntryEditField, EntryEditState};
use crate::types::TimeEntry;
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

/// Truncate `s` to at most `max_chars` Unicode scalar values.
/// Appends `…` if truncation occurred (the ellipsis counts as 1 char toward the limit).
/// Returns the original string if it already fits.
fn truncate_to(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return "…".to_string();
    }
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        // Take max_chars - 1 chars to leave room for the ellipsis
        let truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{}…", truncated)
    }
}

/// Given a remaining character budget, fit `proj_act` and optionally `note` into it.
/// Trims the note first; trims `proj_act` only if it alone overflows.
/// Returns `(proj_act_display, note_display)` — both owned `String`s.
fn fit_proj_act_note(proj_act: &str, note: &str, remaining: usize) -> (String, String) {
    let proj_act_len = proj_act.chars().count();
    let pipe_and_note_len = if note.is_empty() {
        0
    } else {
        3 + note.chars().count()
    };

    if remaining == 0 {
        (String::new(), String::new())
    } else if proj_act_len + pipe_and_note_len <= remaining {
        // Everything fits
        (proj_act.to_string(), note.to_string())
    } else if proj_act_len + 3 < remaining && !note.is_empty() {
        // proj_act fits; trim the note to fill the rest (3 = " | " separator)
        let note_budget = remaining - proj_act_len - 3;
        (proj_act.to_string(), truncate_to(note, note_budget))
    } else if proj_act_len <= remaining {
        // proj_act fills the budget exactly (or no room for note), drop the note
        (proj_act.to_string(), String::new())
    } else {
        // proj_act itself overflows — trim it, drop the note
        (truncate_to(proj_act, remaining), String::new())
    }
}

pub fn build_display_row(
    entry: &TimeEntry,
    is_focused: bool,
    is_overlapping: bool,
    available_width: u16,
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
    let duration_display = if let (Some(start), Some(end)) = (entry.start_time, entry.end_time) {
        let total_minutes = (end - start).whole_minutes();
        let hours = total_minutes / 60;
        let minutes = total_minutes % 60;
        format!("[{:02}h:{:02}m]", hours, minutes)
    } else {
        let total_minutes = (entry.hours * 60.0).round() as i64;
        format!("[{:02}h:{:02}m]", total_minutes / 60, total_minutes % 60)
    };

    let project = &entry.project_name;
    let activity = &entry.activity_name;
    let note = entry.note.as_deref().unwrap_or("");

    // Start time
    let start_str = entry
        .start_time
        .map(|t| {
            let local = to_local_time(t).time();
            format!("{:02}:{:02}", local.hour(), local.minute())
        })
        .unwrap_or_else(|| "XX:XX".to_string());

    // End time
    let end_time_str = if let Some(end_time) = entry.end_time {
        let t = to_local_time(end_time).time();
        format!("{:02}:{:02}", t.hour(), t.minute())
    } else {
        "XX:XX".to_string()
    };

    // Responsive truncation: compute remaining width after fixed prefix.
    // Non-overlapping: "HH:MM - HH:MM " (14) + "[DDh:DDm]" (9) + " | " (3) = 26
    // Overlapping adds "⚠ " (2 chars: symbol + space) = 28
    let prefix_len: usize = if is_overlapping { 28 } else { 26 };
    let remaining = (available_width as usize).saturating_sub(prefix_len);

    let proj_act = format!("{}: {}", project, activity);
    let (proj_act_display, note_display) = fit_proj_act_note(&proj_act, note, remaining);

    // Build styled line with colors
    let mut spans = vec![];

    // Warning prefix for overlapping entries
    if is_overlapping {
        spans.push(Span::styled(
            warning_prefix,
            Style::default().fg(Color::Red),
        ));
    }

    // Show time range — use real times if available, otherwise a dimmed placeholder
    let has_times = entry.start_time.is_some() || entry.end_time.is_some();
    spans.push(Span::styled(
        format!("{} - {} ", start_str, end_time_str),
        Style::default().fg(if has_times {
            time_color
        } else {
            Color::DarkGray
        }),
    ));

    spans.extend(vec![
        // Duration
        Span::styled(duration_display, Style::default().fg(duration_color)),
        // Pipe separator
        Span::styled(" | ", Style::default().fg(Color::DarkGray)),
        // Project - Activity (possibly truncated)
        Span::styled(proj_act_display, Style::default().fg(project_color)),
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

pub fn build_running_timer_display_row(
    app: &App,
    is_focused: bool,
    available_width: u16,
) -> Line<'static> {
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

    let prefix_len: usize = 28; // "▶ " (2) + "HH:MM - HH:MM " (14) + "[DDh:DDm]" (9) + " | " (3)
    let remaining = (available_width as usize).saturating_sub(prefix_len);

    let proj_act = format!("{}: {}", project, activity);
    let (proj_act_display, note_display) = fit_proj_act_note(&proj_act, &note, remaining);

    let text = if note_display.is_empty() {
        format!(
            "▶ {} - HH:MM {} | {}",
            start_str, duration_str, proj_act_display
        )
    } else {
        format!(
            "▶ {} - HH:MM {} | {} | {}",
            start_str, duration_str, proj_act_display, note_display
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
        Span::styled(proj_act_display, Style::default().fg(Color::Cyan)),
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

    spans.push(Span::styled(": ", Style::default().fg(Color::White)));

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
    _entry: &'a TimeEntry,
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
    spans.push(Span::styled(": ", Style::default().fg(Color::White)));

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

use crate::api::ApiClient;
use crate::app::{self, App, TextInput};
use crate::types;
use anyhow::{Context, Result};
use std::time::{Duration, Instant};

use super::action_queue::Action;

/// Apply an active timer fetched from the server into App state.
pub(crate) fn restore_active_timer(app: &mut App, timer: crate::types::ActiveTimerState) {
    let elapsed_secs = (timer.hours * 3600 + timer.minutes * 60 + timer.seconds) as u64;
    app.absolute_start = Some(timer.start_time);
    app.local_start = Some(Instant::now() - Duration::from_secs(elapsed_secs));
    app.timer_state = app::TimerState::Running;
    if let (Some(id), Some(name)) = (timer.project_id, timer.project_name) {
        app.selected_project = Some(crate::types::Project { id, name });
    }
    if let (Some(id), Some(name)) = (timer.activity_id, timer.activity_name) {
        app.selected_activity = Some(crate::types::Activity {
            id,
            name,
            project_id: app
                .selected_project
                .as_ref()
                .map(|p| p.id.clone())
                .unwrap_or_default(),
        });
    }
    if !timer.note.is_empty() {
        app.description_input = app::TextInput::from_str(&timer.note);
        app.description_is_default = false;
    }
}

pub(super) async fn run_action(
    action: Action,
    app: &mut App,
    client: &mut ApiClient,
) -> Result<()> {
    match action {
        Action::SubmitMilltimeReauth => {
            handle_milltime_reauth_submit(app, client).await;
        }
        Action::ApplyProjectSelection {
            had_edit_state,
            saved_selected_project,
            saved_selected_activity,
        } => {
            handle_project_selection_enter(
                app,
                client,
                had_edit_state,
                saved_selected_project,
                saved_selected_activity,
            )
            .await;
        }
        Action::ApplyActivitySelection {
            was_in_edit_mode,
            saved_selected_project,
            saved_selected_activity,
        } => {
            handle_activity_selection_enter(
                app,
                client,
                was_in_edit_mode,
                saved_selected_project,
                saved_selected_activity,
            )
            .await;
        }
        Action::StartTimer => {
            handle_start_timer(app, client).await?;
        }
        Action::SaveTimer => {
            handle_save_timer_with_action(app, client).await?;
        }
        Action::SyncRunningTimerNote { note } => {
            sync_running_timer_note(note, app, client).await;
        }
        Action::SaveHistoryEdit => {
            handle_history_edit_save(app, client).await?;
        }
        Action::SaveThisWeekEdit => {
            handle_this_week_edit_save(app, client).await?;
        }
        Action::LoadHistoryAndOpen => {
            load_history_and_open(app, client).await;
        }
        Action::ConfirmDelete => {
            handle_confirm_delete(app, client).await;
        }
        Action::StopServerTimerAndClear => {
            stop_server_timer_and_clear(app, client).await;
        }
        Action::RefreshHistoryBackground => {
            refresh_history_background(app, client).await;
        }
    }
    Ok(())
}

pub(super) async fn handle_start_timer(app: &mut App, client: &mut ApiClient) -> Result<()> {
    match app.timer_state {
        app::TimerState::Stopped => {
            let project_id = app.selected_project.as_ref().map(|p| p.id.clone());
            let project_name = app.selected_project.as_ref().map(|p| p.name.clone());
            let activity_id = app.selected_activity.as_ref().map(|a| a.id.clone());
            let activity_name = app.selected_activity.as_ref().map(|a| a.name.clone());
            let note = if app.description_input.value.is_empty() {
                None
            } else {
                Some(app.description_input.value.clone())
            };
            if let Err(e) = client
                .start_timer(project_id, project_name, activity_id, activity_name, note)
                .await
            {
                if is_milltime_auth_error(&e) {
                    app.open_milltime_reauth();
                } else {
                    app.set_status(format!("Error starting timer: {}", e));
                }
                return Ok(());
            }
            app.start_timer();
            app.clear_status();
        }
        app::TimerState::Running => {
            app.set_status("Timer already running (Ctrl+S to save)".to_string());
        }
    }
    Ok(())
}

async fn handle_project_selection_enter(
    app: &mut App,
    client: &mut ApiClient,
    had_edit_state: bool,
    saved_selected_project: Option<types::Project>,
    saved_selected_activity: Option<types::Activity>,
) {
    // Fetch activities for the selected project (lazy, cached).
    if let Some(project) = app.selected_project.clone() {
        if !app.activity_cache.contains_key(&project.id) {
            app.is_loading = true;
            match client.get_activities(&project.id).await {
                Ok(activities) => {
                    app.activity_cache.insert(project.id.clone(), activities);
                }
                Err(e) => {
                    app.set_status(format!("Failed to load activities: {}", e));
                }
            }
            app.is_loading = false;
        }

        if let Some(cached) = app.activity_cache.get(&project.id) {
            app.activities = cached.clone();
            app.filtered_activities = cached.clone();
            app.filtered_activity_index = 0;
        }
    }

    if had_edit_state {
        if let Some(project) = app.selected_project.clone() {
            app.update_edit_state_project(project.id.clone(), project.name.clone());
        }
        app.pending_edit_selection_restore =
            Some((saved_selected_project, saved_selected_activity));
    }

    app.navigate_to(app::View::SelectActivity);
}

async fn handle_activity_selection_enter(
    app: &mut App,
    client: &mut ApiClient,
    was_in_edit_mode: bool,
    saved_selected_project: Option<types::Project>,
    saved_selected_activity: Option<types::Activity>,
) {
    if was_in_edit_mode {
        if let Some(activity) = app.selected_activity.clone() {
            app.update_edit_state_activity(activity.id.clone(), activity.name.clone());
        }
        let (restore_project, restore_activity) = app
            .pending_edit_selection_restore
            .take()
            .unwrap_or((saved_selected_project, saved_selected_activity));
        app.selected_project = restore_project;
        app.selected_activity = restore_activity;
        let return_view = app.get_return_view_from_edit();
        app.navigate_to(return_view);
        app.focused_box = app::FocusedBox::Today;
        app.entry_edit_set_focused_field(app::EntryEditField::Activity);
        return;
    }

    app.pending_edit_selection_restore = None;

    if app.timer_state == app::TimerState::Running {
        let project_id = app.selected_project.as_ref().map(|p| p.id.clone());
        let project_name = app.selected_project.as_ref().map(|p| p.name.clone());
        let activity_id = app.selected_activity.as_ref().map(|a| a.id.clone());
        let activity_name = app.selected_activity.as_ref().map(|a| a.name.clone());
        if let Err(e) = client
            .update_active_timer(
                project_id,
                project_name,
                activity_id,
                activity_name,
                None,
                None,
            )
            .await
        {
            app.set_status(format!("Warning: Could not sync project to server: {}", e));
        }
    }
}

fn apply_recent_history(app: &mut App, entries: Vec<types::TimeEntry>) {
    app.update_history(entries);
    app.rebuild_history_list();
}

async fn fetch_recent_history(client: &mut ApiClient) -> Result<Vec<types::TimeEntry>> {
    let today = time::OffsetDateTime::now_utc().date();
    let month_ago = today - time::Duration::days(30);
    client.get_time_entries(month_ago, today).await
}

async fn sync_running_timer_note(note: String, app: &mut App, client: &mut ApiClient) {
    if app.timer_state != app::TimerState::Running {
        return;
    }

    if let Err(e) = client
        .update_active_timer(None, None, None, None, Some(note), None)
        .await
    {
        app.set_status(format!("Warning: Could not sync note to server: {}", e));
    }
}

async fn load_history_and_open(app: &mut App, client: &mut ApiClient) {
    match fetch_recent_history(client).await {
        Ok(entries) => {
            apply_recent_history(app, entries);
            app.navigate_to(app::View::History);
        }
        Err(e) => {
            app.set_status(format!("Error loading history: {}", e));
        }
    }
}

async fn handle_confirm_delete(app: &mut App, client: &mut ApiClient) {
    if let Some(ctx) = app.delete_context.take() {
        let origin = ctx.origin;
        match client.delete_time_entry(&ctx.registration_id).await {
            Ok(()) => {
                app.time_entries
                    .retain(|e| e.registration_id != ctx.registration_id);
                app.rebuild_history_list();
                app.set_status("Entry deleted".to_string());
            }
            Err(e) => {
                app.set_status(format!("Delete failed: {}", e));
            }
        }
        match origin {
            app::DeleteOrigin::Timer => app.navigate_to(app::View::Timer),
            app::DeleteOrigin::History => app.navigate_to(app::View::History),
        }
    }
}

async fn stop_server_timer_and_clear(app: &mut App, client: &mut ApiClient) {
    if app.timer_state == app::TimerState::Running {
        if let Err(e) = client.stop_timer().await {
            app.set_status(format!("Warning: Could not stop server timer: {}", e));
        }
    }
    app.clear_timer();
}

async fn refresh_history_background(app: &mut App, client: &mut ApiClient) {
    match fetch_recent_history(client).await {
        Ok(entries) => {
            apply_recent_history(app, entries);
        }
        Err(e) if is_milltime_auth_error(&e) => {
            app.open_milltime_reauth();
        }
        Err(_) => {}
    }
}

pub(super) async fn handle_save_timer_with_action(
    app: &mut App,
    client: &mut ApiClient,
) -> Result<()> {
    // Handle Cancel first
    if app.selected_save_action == app::SaveAction::Cancel {
        app.navigate_to(app::View::Timer);
        return Ok(());
    }

    let duration = app.elapsed_duration();
    let note = if app.description_input.value.is_empty() {
        None
    } else {
        Some(app.description_input.value.clone())
    };

    let project_display = app.current_project_name();
    let activity_display = app.current_activity_name();

    // Save the active timer to Milltime
    match client.save_timer(note.clone()).await {
        Ok(()) => {
            let hours = duration.as_secs() / 3600;
            let minutes = (duration.as_secs() % 3600) / 60;
            let seconds = duration.as_secs() % 60;
            let duration_str = format!("{:02}:{:02}:{:02}", hours, minutes, seconds);

            // Refresh history
            if let Ok(entries) = fetch_recent_history(client).await {
                apply_recent_history(app, entries);
            }

            match app.selected_save_action {
                app::SaveAction::ContinueSameProject => {
                    app.description_input.clear();
                    app.description_is_default = true;
                    // Start a new server-side timer with same project/activity
                    let project_id = app.selected_project.as_ref().map(|p| p.id.clone());
                    let project_name = app.selected_project.as_ref().map(|p| p.name.clone());
                    let activity_id = app.selected_activity.as_ref().map(|a| a.id.clone());
                    let activity_name = app.selected_activity.as_ref().map(|a| a.name.clone());
                    if let Err(e) = client
                        .start_timer(project_id, project_name, activity_id, activity_name, None)
                        .await
                    {
                        app.set_status(format!("Saved but could not restart timer: {}", e));
                    } else {
                        app.start_timer();
                        app.set_status(format!(
                            "Saved {} to {} / {}",
                            duration_str, project_display, activity_display
                        ));
                    }
                }
                app::SaveAction::ContinueNewProject => {
                    app.selected_project = None;
                    app.selected_activity = None;
                    app.description_input.clear();
                    app.description_is_default = true;
                    // Start new timer with no project yet
                    if let Err(e) = client.start_timer(None, None, None, None, None).await {
                        app.set_status(format!("Saved but could not restart timer: {}", e));
                    } else {
                        app.start_timer();
                        app.set_status(format!(
                            "Saved {}. Timer started. Press P to select project.",
                            duration_str
                        ));
                    }
                }
                app::SaveAction::SaveAndStop => {
                    app.timer_state = app::TimerState::Stopped;
                    app.absolute_start = None;
                    app.local_start = None;
                    if let Some(idx) = app.focused_this_week_index {
                        app.focused_this_week_index = if idx == 0 {
                            None
                        } else {
                            Some(idx.saturating_sub(1))
                        };
                    }
                    app.set_status(format!(
                        "Saved {} to {} / {}",
                        duration_str, project_display, activity_display
                    ));
                }
                app::SaveAction::Cancel => unreachable!(),
            }

            app.navigate_to(app::View::Timer);
        }
        Err(e) => {
            if is_milltime_auth_error(&e) {
                app.open_milltime_reauth();
            } else {
                app.set_status(format!("Error saving timer: {}", e));
            }
            app.navigate_to(app::View::Timer);
        }
    }

    Ok(())
}

// Helper functions for edit mode

/// Handle Enter key in edit mode - open modal for Project/Activity/Note or move to next field
pub(super) fn handle_entry_edit_enter(app: &mut App) {
    // Extract the data we need first to avoid borrow conflicts
    let action = {
        if let Some(state) = app.current_edit_state() {
            match state.focused_field {
                app::EntryEditField::Project => Some(('P', None)),
                app::EntryEditField::Activity => {
                    if state.project_id.is_some() {
                        Some(('A', None))
                    } else {
                        app.set_status("Please select a project first".to_string());
                        None
                    }
                }
                app::EntryEditField::Note => {
                    let note = state.note.value.clone();
                    Some(('N', Some(note)))
                }
                app::EntryEditField::StartTime | app::EntryEditField::EndTime => {
                    // Move to next field (like Tab)
                    app.entry_edit_next_field();
                    None
                }
            }
        } else {
            None
        }
    };

    // Now perform actions that don't require the borrow
    if let Some((action, note)) = action {
        match action {
            'P' => {
                app.navigate_to(app::View::SelectProject);
            }
            'A' => {
                app.navigate_to(app::View::SelectActivity);
            }
            'N' => {
                // Save running timer's note before overwriting with entry's note
                app.saved_timer_note = Some(app.description_input.value.clone());
                // Set description_input from the edit state before navigating
                if let Some(n) = note {
                    app.description_input = TextInput::from_str(&n);
                }
                // Open description editor
                app.navigate_to(app::View::EditDescription);
            }
            _ => {}
        }
    }
}

/// Save changes from This Week edit mode to database
pub(super) async fn handle_this_week_edit_save(
    app: &mut App,
    client: &mut ApiClient,
) -> Result<()> {
    // Running timer edits don't touch the DB
    if app
        .this_week_edit_state
        .as_ref()
        .map(|s| s.registration_id.is_empty())
        == Some(true)
    {
        handle_running_timer_edit_save(app, client).await;
        return Ok(());
    }

    let Some(state) = app.this_week_edit_state.take() else {
        return Ok(());
    };
    app.exit_this_week_edit_mode();
    if let Err(e) = handle_saved_entry_edit_save(state, app, client).await {
        if is_milltime_auth_error(&e) {
            app.open_milltime_reauth();
        } else {
            app.set_status(format!("Error saving entry: {}", e));
        }
    }
    Ok(())
}

/// Apply edits from This Week edit mode back to the live running timer (no DB write).
/// Called when registration_id is empty (sentinel for the running timer).
async fn handle_running_timer_edit_save(app: &mut App, client: &mut ApiClient) {
    let Some(state) = app.this_week_edit_state.take() else {
        return;
    };

    // Parse start time input
    let start_parts: Vec<&str> = state.start_time_input.split(':').collect();
    if start_parts.len() != 2 {
        app.set_status("Error: Invalid time format".to_string());
        return;
    }
    let Ok(start_hours) = start_parts[0].parse::<u8>() else {
        app.set_status("Error: Invalid start hour".to_string());
        return;
    };
    let Ok(start_mins) = start_parts[1].parse::<u8>() else {
        app.set_status("Error: Invalid start minute".to_string());
        return;
    };

    // Build new absolute_start: today's local date + typed HH:MM, converted to UTC
    let local_offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);
    let today = time::OffsetDateTime::now_utc()
        .to_offset(local_offset)
        .date();
    let Ok(new_time) = time::Time::from_hms(start_hours, start_mins, 0) else {
        app.set_status("Error: Invalid start time".to_string());
        return;
    };
    let new_start = time::OffsetDateTime::new_in_offset(today, new_time, local_offset);

    // Reject if new start is in the future
    if new_start > time::OffsetDateTime::now_utc() {
        app.set_status("Error: Start time cannot be in the future".to_string());
        // Restore edit state so the user can correct it
        app.this_week_edit_state = Some(state);
        return;
    }

    // Write back to App fields
    app.absolute_start = Some(new_start.to_offset(time::UtcOffset::UTC));

    // Recalculate local_start so elapsed_duration() reflects the new start time
    let now_utc = time::OffsetDateTime::now_utc();
    let elapsed_secs = (now_utc - new_start.to_offset(time::UtcOffset::UTC))
        .whole_seconds()
        .max(0) as u64;
    app.local_start =
        Some(std::time::Instant::now() - std::time::Duration::from_secs(elapsed_secs));

    app.selected_project = state
        .project_id
        .zip(state.project_name)
        .map(|(id, name)| types::Project { id, name });
    app.selected_activity = state
        .activity_id
        .zip(state.activity_name)
        .map(|(id, name)| types::Activity {
            id,
            name,
            project_id: String::new(),
        });
    app.description_input = TextInput::from_str(&state.note.value);

    app.set_status("Running timer updated".to_string());

    // Sync updated start time / project / activity / note to server
    let project_id = app.selected_project.as_ref().map(|p| p.id.clone());
    let project_name = app.selected_project.as_ref().map(|p| p.name.clone());
    let activity_id = app.selected_activity.as_ref().map(|a| a.id.clone());
    let activity_name = app.selected_activity.as_ref().map(|a| a.name.clone());
    let note = if app.description_input.value.is_empty() {
        None
    } else {
        Some(app.description_input.value.clone())
    };
    if let Err(e) = client
        .update_active_timer(
            project_id,
            project_name,
            activity_id,
            activity_name,
            note,
            app.absolute_start,
        )
        .await
    {
        app.set_status(format!("Warning: Could not sync timer to server: {}", e));
    }
}

/// Save changes from History edit mode to database
pub(super) async fn handle_history_edit_save(app: &mut App, client: &mut ApiClient) -> Result<()> {
    let Some(state) = app.history_edit_state.take() else {
        return Ok(());
    };
    app.exit_history_edit_mode();
    if let Err(e) = handle_saved_entry_edit_save(state, app, client).await {
        if is_milltime_auth_error(&e) {
            app.open_milltime_reauth();
        } else {
            app.set_status(format!("Error saving entry: {}", e));
        }
    }
    Ok(())
}

/// Shared helper: save edits to a completed (non-running) timer history entry via the API.
async fn handle_saved_entry_edit_save(
    state: app::EntryEditState,
    app: &mut App,
    client: &mut ApiClient,
) -> Result<()> {
    // Look up the original entry from history
    let entry = match app
        .time_entries
        .iter()
        .find(|e| e.registration_id == state.registration_id)
    {
        Some(e) => e.clone(),
        None => {
            app.set_status("Error: Entry not found in history".to_string());
            return Ok(());
        }
    };

    // registration_id is always present on TimeEntry
    let registration_id = entry.registration_id.clone();

    // Parse start / end times (HH:MM) on the entry's original local date
    let local_offset = time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC);

    // Parse entry.date ("YYYY-MM-DD") to get the calendar date
    let entry_date = app::parse_date_str(&entry.date)
        .ok_or_else(|| anyhow::anyhow!("Unexpected date format: {}", entry.date))?;

    let parse_hhmm = |s: &str| -> Result<time::Time> {
        let parts: Vec<&str> = s.split(':').collect();
        anyhow::ensure!(parts.len() == 2, "Expected HH:MM format, got {:?}", s);
        let h: u8 = parts[0].parse().context("Invalid hour")?;
        let m: u8 = parts[1].parse().context("Invalid minute")?;
        time::Time::from_hms(h, m, 0).map_err(|e| anyhow::anyhow!("Invalid time: {}", e))
    };

    let start_local = time::OffsetDateTime::new_in_offset(
        entry_date,
        parse_hhmm(&state.start_time_input)?,
        local_offset,
    );
    let end_local = time::OffsetDateTime::new_in_offset(
        entry_date,
        parse_hhmm(&state.end_time_input)?,
        local_offset,
    );

    anyhow::ensure!(end_local > start_local, "End time must be after start time");

    // Compute reg_day and week_number from the entry date
    let reg_day = format!(
        "{:04}-{:02}-{:02}",
        entry_date.year(),
        entry_date.month() as u8,
        entry_date.day()
    );
    let week_number = entry_date.iso_week() as i32;

    // Determine delta fields (only set if project/activity changed)
    let original_project_id = if state.project_id.as_deref() != Some(entry.project_id.as_str()) {
        Some(entry.project_id.as_str())
    } else {
        None
    };
    let original_activity_id = if state.activity_id.as_deref() != Some(entry.activity_id.as_str()) {
        Some(entry.activity_id.as_str())
    } else {
        None
    };

    let project_id = state.project_id.as_deref().unwrap_or("");
    let project_name = state.project_name.as_deref().unwrap_or("");
    let activity_id = state.activity_id.as_deref().unwrap_or("");
    let activity_name = state.activity_name.as_deref().unwrap_or("");
    let user_note = &state.note.value;

    client
        .edit_time_entry(
            &registration_id,
            project_id,
            project_name,
            activity_id,
            activity_name,
            start_local.to_offset(time::UtcOffset::UTC),
            end_local.to_offset(time::UtcOffset::UTC),
            &reg_day,
            week_number,
            user_note,
            original_project_id,
            original_activity_id,
        )
        .await?;

    // Reload history to reflect the changes
    match fetch_recent_history(client).await {
        Ok(entries) => {
            apply_recent_history(app, entries);
        }
        Err(e) => {
            app.set_status(format!(
                "Entry updated (warning: could not reload history: {})",
                e
            ));
            return Ok(());
        }
    }

    app.set_status("Entry updated".to_string());
    Ok(())
}

/// Attempt Milltime re-authentication with the credentials from the overlay.
/// On success: updates cookies and closes the overlay.
/// On failure: sets the error message on the overlay so the user can retry.
pub(super) async fn handle_milltime_reauth_submit(app: &mut App, client: &mut ApiClient) {
    let (username, password) = match app.milltime_reauth_credentials() {
        Some(creds) => creds,
        None => return,
    };
    if username.is_empty() {
        app.milltime_reauth_set_error("Username is required".to_string());
        return;
    }
    match client.authenticate(&username, &password).await {
        Ok(()) => {
            app.close_milltime_reauth();
            app.set_status("Milltime re-authenticated successfully".to_string());
        }
        Err(e) => {
            app.milltime_reauth_set_error(format!("Authentication failed: {}", e));
        }
    }
}

/// Returns true when an error looks like a Milltime authentication failure.
/// Used to decide whether to open the re-auth overlay.
pub(super) fn is_milltime_auth_error(e: &anyhow::Error) -> bool {
    let msg = e.to_string().to_lowercase();
    msg.contains("unauthorized") || msg.contains("authenticate") || msg.contains("milltime")
}

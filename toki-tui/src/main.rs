mod api;
mod app;
mod git;
mod test_data;
mod ui;

use anyhow::Result;
use app::App;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env.tui
    dotenvy::from_filename(".env.tui").ok();

    // Get database URL from environment
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://postgres:password@localhost:5432/toki_tui_dev".to_string());

    println!("🔍 Connecting to database: {}", database_url);
    println!("⚠️  Using ISOLATED dev database (toki_tui_dev)");
    println!("✅ Your production database is safe!\n");

    // Connect to database
    let db = match api::Database::new(&database_url).await {
        Ok(db) => {
            println!("✅ Connected to database successfully!\n");
            db
        }
        Err(e) => {
            eprintln!("❌ Failed to connect to database: {}", e);
            eprintln!("\n💡 Make sure you've created the dev database:");
            eprintln!("   1. Create database: createdb -U postgres toki_tui_dev");
            eprintln!("   2. Run migrations: just init-tui-db");
            eprintln!("\n   Or use the justfile commands.");
            return Err(e);
        }
    };

    // For demo purposes, use user_id = 1
    // In production, this would come from authentication
    let user_id = 1;

    // Create app state
    let mut app = App::new(user_id);

    // Load timer history (fetch more entries for month coverage)
    match db.get_timer_history(user_id, 500).await {
        Ok(history) => {
            app.update_history(history);
            app.rebuild_history_list();
        }
        Err(e) => {
            eprintln!("Warning: Could not load history: {}", e);
        }
    }

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the app
    let res = run_app(&mut terminal, &mut app, &db).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    println!("\n👋 Goodbye!");

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    db: &api::Database,
) -> Result<()> {
    loop {
        terminal.draw(|f| ui::render(f, app))?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match &app.current_view {
                    app::View::SelectProject => {
                        // Save edit state and running timer state before any selection
                        let had_edit_state = app.is_in_edit_mode();
                        // Save running timer's project/activity
                        let saved_selected_project = app.selected_project.clone();
                        let saved_selected_activity = app.selected_activity.clone();
                        
                        match key.code {
                            KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                app.search_input_clear();
                            }
                            KeyCode::Tab => {
                                app.selection_list_focused = true;
                            }
                            KeyCode::BackTab => {
                                app.selection_list_focused = false;
                            }
                            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) && c != 'q' && c != 'Q' => {
                                if app.selection_list_focused && c == 'j' {
                                    if app.filtered_project_index + 1 >= app.filtered_projects.len() {
                                        app.selection_list_focused = false;
                                    } else {
                                        app.select_next();
                                    }
                                } else if app.selection_list_focused && c == 'k' {
                                    if app.filtered_project_index == 0 {
                                        app.selection_list_focused = false;
                                    } else {
                                        app.select_previous();
                                    }
                                } else if !app.selection_list_focused {
                                    app.search_input_char(c);
                                }
                            }
                            KeyCode::Backspace => {
                                app.search_input_backspace();
                            }
                            KeyCode::Up => {
                                if app.selection_list_focused && app.filtered_project_index == 0 {
                                    app.selection_list_focused = false;
                                } else {
                                    app.select_previous();
                                }
                            }
                            KeyCode::Down => {
                                if app.selection_list_focused && app.filtered_project_index + 1 >= app.filtered_projects.len() {
                                    app.selection_list_focused = false;
                                } else {
                                    app.select_next();
                                }
                            }
                            KeyCode::Enter => {
                                app.confirm_selection();
                                // If we were in edit mode, restore with selected project AND restore running timer state
                                if had_edit_state {
                                    if let Some(project) = app.selected_project.clone() {
                                        app.update_edit_state_project(project.id.clone(), project.name.clone());
                                    }
                                    // Restore running timer's project/activity
                                    app.selected_project = saved_selected_project;
                                    app.selected_activity = saved_selected_activity;
                                }
                                // Auto-show activity selection
                                app.navigate_to(app::View::SelectActivity);
                            }
                            KeyCode::Esc => app.cancel_selection(),
                            KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
                            _ => {}
                        }
                    }
                    app::View::SelectActivity => {
                        // Save edit state and running timer state before any selection
                        let was_in_edit_mode = app.is_in_edit_mode();
                        let saved_selected_project = app.selected_project.clone();
                        let saved_selected_activity = app.selected_activity.clone();
                        
                        match key.code {
                            KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                app.activity_search_input_clear();
                            }
                            KeyCode::Tab => {
                                app.selection_list_focused = true;
                            }
                            KeyCode::BackTab => {
                                app.selection_list_focused = false;
                            }
                            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) && c != 'q' && c != 'Q' => {
                                if app.selection_list_focused && c == 'j' {
                                    if app.filtered_activity_index + 1 >= app.filtered_activities.len() {
                                        app.selection_list_focused = false;
                                    } else {
                                        app.select_next();
                                    }
                                } else if app.selection_list_focused && c == 'k' {
                                    if app.filtered_activity_index == 0 {
                                        app.selection_list_focused = false;
                                    } else {
                                        app.select_previous();
                                    }
                                } else if !app.selection_list_focused {
                                    app.activity_search_input_char(c);
                                }
                            }
                            KeyCode::Backspace => {
                                app.activity_search_input_backspace();
                            }
                            KeyCode::Up => {
                                if app.selection_list_focused && app.filtered_activity_index == 0 {
                                    app.selection_list_focused = false;
                                } else {
                                    app.select_previous();
                                }
                            }
                            KeyCode::Down => {
                                if app.selection_list_focused && app.filtered_activity_index + 1 >= app.filtered_activities.len() {
                                    app.selection_list_focused = false;
                                } else {
                                    app.select_next();
                                }
                            }
                            KeyCode::Enter => {
                                app.confirm_selection();
                                
                                // If we were in edit mode, restore edit state with selected activity AND restore running timer state
                                if was_in_edit_mode {
                                    if let Some(activity) = app.selected_activity.clone() {
                                        app.update_edit_state_activity(activity.id.clone(), activity.name.clone());
                                    }
                                    // Restore running timer's project/activity
                                    app.selected_project = saved_selected_project;
                                    app.selected_activity = saved_selected_activity;
                                    // Navigate back to the appropriate view
                                    let return_view = app.get_return_view_from_edit();
                                    app.navigate_to(return_view);
                                    if return_view == app::View::Timer {
                                        app.focused_box = app::FocusedBox::Today;
                                        app.entry_edit_set_focused_field(app::EntryEditField::Activity);
                                    } else {
                                        app.focused_box = app::FocusedBox::Today; // Not used in History view but keep consistent
                                        app.entry_edit_set_focused_field(app::EntryEditField::Activity);
                                    }
                                }
                            }
                            KeyCode::Esc => app.cancel_selection(),
                            KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
                            _ => {}
                        }
                    }
                    app::View::EditDescription => {
                        let was_in_edit_mode = app.is_in_edit_mode();

                        // CWD change mode takes priority
                        if app.cwd_input.is_some() {
                            match key.code {
                                KeyCode::Esc => app.cancel_cwd_change(),
                                KeyCode::Enter => {
                                    if let Err(e) = app.confirm_cwd_change() {
                                        app.status_message = Some(e);
                                    }
                                }
                                KeyCode::Tab => app.cwd_tab_complete(),
                                KeyCode::Backspace => app.cwd_input_backspace(),
                                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    app.cwd_input_char(c);
                                }
                                _ => {}
                            }
                        } else if app.git_mode {
                            // Second key of Ctrl+G sequence
                            match key.code {
                                KeyCode::Char('1') => app.paste_git_branch_raw(),
                                KeyCode::Char('2') => app.paste_git_branch_parsed(),
                                KeyCode::Char('3') => app.paste_git_last_commit(),
                                _ => app.exit_git_mode(), // any other key cancels git mode
                            }
                        } else {
                            match key.code {
                                KeyCode::Char('x') | KeyCode::Char('X')
                                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    app.description_input.clear();
                                }
                                KeyCode::Char('g') | KeyCode::Char('G')
                                    if key.modifiers.contains(KeyModifiers::CONTROL)
                                        && app.git_context.branch.is_some() =>
                                {
                                    app.enter_git_mode();
                                }
                                KeyCode::Char('d') | KeyCode::Char('D')
                                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    app.begin_cwd_change();
                                }
                                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    app.input_char(c);
                                }
                                KeyCode::Backspace => app.input_backspace(),
                                KeyCode::Enter => {
                                    if was_in_edit_mode {
                                        app.update_edit_state_note(app.description_input.clone());
                                        if let Some(saved_note) = app.saved_timer_note.take() {
                                            app.description_input = saved_note;
                                        }
                                        let return_view = app.get_return_view_from_edit();
                                        app.navigate_to(return_view);
                                        if return_view == app::View::Timer {
                                            app.focused_box = app::FocusedBox::Today;
                                        }
                                    } else {
                                        app.confirm_description();
                                    }
                                }
                                KeyCode::Esc => {
                                    if was_in_edit_mode {
                                        if let Some(saved_note) = app.saved_timer_note.take() {
                                            app.description_input = saved_note;
                                        }
                                        let return_view = app.get_return_view_from_edit();
                                        app.navigate_to(return_view);
                                        if return_view == app::View::Timer {
                                            app.focused_box = app::FocusedBox::Today;
                                        }
                                    } else {
                                        app.cancel_selection();
                                    }
                                }
                                KeyCode::Char('q') | KeyCode::Char('Q')
                                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    app.quit();
                                }
                                _ => {}
                            }
                        }
                    }
                    app::View::SaveAction => {
                        match key.code {
                            KeyCode::Char('1') => {
                                app.select_save_action_by_number(1);
                                handle_save_timer_with_action(app, &db).await?;
                            }
                            KeyCode::Char('2') => {
                                app.select_save_action_by_number(2);
                                handle_save_timer_with_action(app, &db).await?;
                            }
                            KeyCode::Char('3') => {
                                app.select_save_action_by_number(3);
                                handle_save_timer_with_action(app, &db).await?;
                            }
                            KeyCode::Char('4') | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                                // Cancel - return to timer view
                                app.navigate_to(app::View::Timer);
                            }
                            KeyCode::Up | KeyCode::Char('k') => app.select_previous_save_action(),
                            KeyCode::Down | KeyCode::Char('j') => app.select_next_save_action(),
                            KeyCode::Enter => {
                                handle_save_timer_with_action(app, &db).await?;
                            }
                            _ => {}
                        }
                    }
                    app::View::History => {
                        // Check if we're in edit mode
                        if app.history_edit_state.is_some() {
                            match key.code {
                                // Tab: next field
                                KeyCode::Tab => {
                                    app.entry_edit_next_field();
                                }
                                KeyCode::BackTab => {
                                    app.entry_edit_prev_field();
                                }
                                // Arrow keys: navigate fields
                                KeyCode::Down | KeyCode::Char('j') => {
                                    app.entry_edit_next_field();
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    app.entry_edit_prev_field();
                                }
                                KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => {
                                    app.entry_edit_next_field();
                                }
                                KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => {
                                    app.entry_edit_prev_field();
                                }
                                // Number keys for time input
                                KeyCode::Char(c) if c.is_ascii_digit() => {
                                    app.entry_edit_input_char(c);
                                }
                                KeyCode::Backspace => {
                                    app.entry_edit_backspace();
                                }
                                // Enter: edit field or move to next field for times
                                KeyCode::Enter => {
                                    if let Some(state) = &app.history_edit_state {
                                        match state.focused_field {
                                            app::EntryEditField::StartTime | app::EntryEditField::EndTime => {
                                                // Move to next field
                                                app.entry_edit_next_field();
                                            }
                                            _ => {
                                                handle_entry_edit_enter(app);
                                            }
                                        }
                                    }
                                }
                                // Ctrl+X: Clear time field (when focused on time input)
                                KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    if let Some(state) = &app.history_edit_state {
                                        match state.focused_field {
                                            app::EntryEditField::StartTime | app::EntryEditField::EndTime => {
                                                app.entry_edit_clear_time();
                                            }
                                            _ => {}
                                        }
                                    }
                                }
                                // Escape: save and exit edit mode
                                KeyCode::Esc => {
                                    if let Some(error) = app.entry_edit_validate() {
                                        app.entry_edit_revert_invalid_times();
                                        app.set_status(format!("Edit cancelled: {}", error));
                                        app.exit_history_edit_mode();
                                    } else {
                                        handle_history_edit_save(app, db).await?;
                                    }
                                }
                                // P: select project
                                KeyCode::Char('p') | KeyCode::Char('P') => {
                                    app.navigate_to(app::View::SelectProject);
                                }
                                // Q: quit
                                KeyCode::Char('q') | KeyCode::Char('Q') => {
                                    app.quit();
                                }
                                _ => {}
                            }
                        } else {
                            // Not in edit mode
                            match key.code {
                                KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
                                KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                                KeyCode::Enter => {
                                    // Enter edit mode
                                    app.enter_history_edit_mode();
                                }
                                KeyCode::Char('h') | KeyCode::Char('H') | KeyCode::Esc => {
                                    app.navigate_to(app::View::Timer);
                                }
                                KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
                                _ => {}
                            }
                        }
                    }
                    app::View::Statistics => {
                        match key.code {
                            KeyCode::Char('s') | KeyCode::Char('S')
                            | KeyCode::Esc => {
                                app.navigate_to(app::View::Timer);
                            }
                            KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
                            _ => {}
                        }
                    }
                    app::View::Timer => {
                        match key.code {
                            // Quit
                            KeyCode::Char('q') | KeyCode::Char('Q') => {
                                app.quit();
                            }
                            // Ctrl+C also quits
                            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                app.quit();
                            }
                            // Ctrl+S: Save & continue
                            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                // Validate first
                                if app.timer_state == app::TimerState::Stopped {
                                    app.set_status("No active timer to save".to_string());
                                } else if !app.has_project_activity() {
                                    app.set_status("Cannot save: Please select Project / Activity first (press P)".to_string());
                                } else {
                                    // Show save action dialog
                                    app.navigate_to(app::View::SaveAction);
                                }
                            }
                            // Tab: Navigate forward between boxes (or next field in edit mode)
                            KeyCode::Tab => {
                                if app.this_week_edit_state.is_some() {
                                    app.entry_edit_next_field();
                                } else {
                                    app.focus_next();
                                }
                            }
                            // Shift+Tab (BackTab): Navigate backward between boxes (or prev field in edit mode)
                            KeyCode::BackTab => {
                                if app.this_week_edit_state.is_some() {
                                    app.entry_edit_prev_field();
                                } else {
                                    app.focus_previous();
                                }
                            }
                            // Arrow down / j: Move down (next row in This Week, or next field in edit mode)
                            KeyCode::Down | KeyCode::Char('j') => {
                                if app.this_week_edit_state.is_some() {
                                    app.entry_edit_next_field();
                                } else if app.focused_box == app::FocusedBox::Today {
                                    app.this_week_focus_down();
                                } else {
                                    app.focus_next();
                                }
                            }
                            // Arrow up / k: Move up (prev row in This Week, or prev field in edit mode)
                            KeyCode::Up | KeyCode::Char('k') => {
                                if app.this_week_edit_state.is_some() {
                                    app.entry_edit_prev_field();
                                } else if app.focused_box == app::FocusedBox::Today {
                                    app.this_week_focus_up();
                                } else {
                                    app.focus_previous();
                                }
                            }
                            // Arrow right / l: Next field (edit mode only)
                            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => {
                                if app.this_week_edit_state.is_some() {
                                    app.entry_edit_next_field();
                                }
                            }
                            // Arrow left / h: Prev field (edit mode only) or open History
                            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => {
                                if app.this_week_edit_state.is_some() {
                                    app.entry_edit_prev_field();
                                } else if app.this_week_edit_state.is_none() {
                                    // Open History view when not in edit mode
                                    match db.get_timer_history(app.user_id, 500).await {
                                        Ok(history) => {
                                            app.update_history(history);
                                            app.rebuild_history_list();
                                            app.navigate_to(app::View::History);
                                        }
                                        Err(e) => {
                                            app.set_status(format!("Error loading history: {}", e));
                                        }
                                    }
                                }
                            }
                            // Enter: activate focused box or move to next field in edit mode
                            KeyCode::Enter => {
                                if app.this_week_edit_state.is_some() {
                                    // Check if focused on time field - move to next field
                                    if let Some(state) = &app.this_week_edit_state {
                                        match state.focused_field {
                                            app::EntryEditField::StartTime | app::EntryEditField::EndTime => {
                                                // Move to next field
                                                app.entry_edit_next_field();
                                            }
                                            _ => {
                                                // In edit mode, Enter on Project/Activity/Note opens modal
                                                handle_entry_edit_enter(app);
                                            }
                                        }
                                    }
                                } else {
                                    match app.focused_box {
                                        app::FocusedBox::Timer => {
                                            // Start timer when Timer box is focused
                                            handle_start_timer(app)?;
                                        }
                                        app::FocusedBox::Today => {
                                            // If no entry selected, default to first entry
                                            if app.focused_this_week_index.is_none() && !app.this_week_history().is_empty() {
                                                app.focused_this_week_index = Some(0);
                                            }
                                            app.enter_this_week_edit_mode();
                                        }
                                        _ => {
                                            app.activate_focused_box();
                                        }
                                    }
                                }
                            }
                            // Number keys for time input in edit mode
                            KeyCode::Char(c) if app.this_week_edit_state.is_some() && c.is_ascii_digit() => {
                                app.entry_edit_input_char(c);
                            }
                            KeyCode::Backspace => {
                                if app.this_week_edit_state.is_some() {
                                    app.entry_edit_backspace();
                                }
                            }
                            // Escape to exit edit mode
                            KeyCode::Esc => {
                                if app.this_week_edit_state.is_some() {
                                    // Check validation
                                    if let Some(error) = app.entry_edit_validate() {
                                        // Revert invalid times and show error
                                        app.entry_edit_revert_invalid_times();
                                        app.set_status(format!("Edit cancelled: {}", error));
                                        app.exit_this_week_edit_mode();
                                        app.focused_box = app::FocusedBox::Today;
                                    } else {
                                        // Save changes via API
                                        handle_this_week_edit_save(app, db).await?;
                                    }
                                } else {
                                    app.focused_box = app::FocusedBox::Timer;
                                    app.focused_this_week_index = None;
                                }
                            }
                            // Space: Start timer or Save & Stop
                            KeyCode::Char(' ') => {
                                match app.timer_state {
                                    app::TimerState::Stopped => {
                                        handle_start_timer(app)?;
                                    }
                                    app::TimerState::Running => {
                                        if !app.has_project_activity() {
                                            app.set_status("Cannot save: Please select Project / Activity first (press P)".to_string());
                                        } else {
                                            // Save & stop directly without showing dialog
                                            app.selected_save_action = app::SaveAction::SaveAndStop;
                                            handle_save_timer_with_action(app, db).await?;
                                        }
                                    }
                                }
                            }
                            // P: Select project
                            KeyCode::Char('p') | KeyCode::Char('P') => {
                                app.navigate_to(app::View::SelectProject);
                            }
                            // N: Edit note
                            KeyCode::Char('n') | KeyCode::Char('N') => {
                                app.navigate_to(app::View::EditDescription);
                            }
                            // T: Toggle timer size
                            KeyCode::Char('t') | KeyCode::Char('T') => {
                                app.toggle_timer_size();
                            }
                            // S: Open Statistics view (unmodified only — Ctrl+S is save)
                            KeyCode::Char('s') | KeyCode::Char('S')
                                if !key.modifiers.contains(KeyModifiers::CONTROL) =>
                            {
                                app.navigate_to(app::View::Statistics);
                            }
                            // Ctrl+X: Clear time field (when in edit mode on time input) or clear timer
                            KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                if app.this_week_edit_state.is_some() {
                                    // In edit mode - clear time field if focused on time input
                                    if let Some(state) = &app.this_week_edit_state {
                                        match state.focused_field {
                                            app::EntryEditField::StartTime | app::EntryEditField::EndTime => {
                                                app.entry_edit_clear_time();
                                            }
                                            _ => {}
                                        }
                                    }
                                } else {
                                    // Not in edit mode - clear timer
                                    app.clear_timer();
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        if !app.running {
            break;
        }
    }

    Ok(())
}

fn handle_start_timer(app: &mut App) -> Result<()> {
    match app.timer_state {
        app::TimerState::Stopped => {
            app.start_timer();
            // Clear status to show contextual message
            app.clear_status();
        }
        app::TimerState::Running => {
            app.set_status("Timer already running (Ctrl+S to save)".to_string());
        }
    }
    Ok(())
}

async fn handle_save_timer_with_action(app: &mut App, db: &api::Database) -> Result<()> {
    // Handle Cancel first
    if app.selected_save_action == app::SaveAction::Cancel {
        app.navigate_to(app::View::Timer);
        return Ok(());
    }

    // Validate and save
    if let Some(start_time) = app.absolute_start {
        let end_time = time::OffsetDateTime::now_utc();
        let duration = app.elapsed_duration();
        
        let project_id = app.selected_project.as_ref().map(|p| p.id.clone());
        let project_name = Some(app.current_project_name());
        let activity_id = app.selected_activity.as_ref().map(|a| a.id.clone());
        let activity_name = Some(app.current_activity_name());
        let note = Some(app.description_input.clone());
        
        // Save to database
        match db.save_timer_entry(
            app.user_id,
            start_time,
            end_time,
            project_id,
            project_name.clone(),
            activity_id,
            activity_name.clone(),
            note,
        ).await {
            Ok(_) => {
                // Format status message
                let hours = duration.as_secs() / 3600;
                let minutes = (duration.as_secs() % 3600) / 60;
                let seconds = duration.as_secs() % 60;
                let duration_str = format!("{:02}:{:02}:{:02}", hours, minutes, seconds);
                
                let project_display = project_name.unwrap_or_else(|| "[None]".to_string());
                let activity_display = activity_name.unwrap_or_else(|| "[None]".to_string());
                
                // Refresh history
                if let Ok(history) = db.get_timer_history(app.user_id, 50).await {
                    app.update_history(history);
                }
                
                // Handle action-specific behavior
                match app.selected_save_action {
                    app::SaveAction::ContinueSameProject => {
                        // Keep project/activity, clear description
                        app.description_input.clear();
                        app.description_is_default = true;
                        app.start_timer();
                        app.set_status(format!(
                            "Saved {} to {} / {}",
                            duration_str, project_display, activity_display
                        ));
                    }
                    app::SaveAction::ContinueNewProject => {
                        // Clear everything
                        app.selected_project = None;
                        app.selected_activity = None;
                        app.description_input.clear();
                        app.description_is_default = true;
                        app.start_timer();
                        app.set_status(format!(
                            "Saved {}. Timer started. Press P to select project.",
                            duration_str
                        ));
                    }
                    app::SaveAction::SaveAndStop => {
                        // Stop timer, keep everything
                        app.timer_state = app::TimerState::Stopped;
                        app.absolute_start = None;
                        app.local_start = None;
                        app.set_status(format!(
                            "Saved {} to {} / {}",
                            duration_str, project_display, activity_display
                        ));
                    }
                    app::SaveAction::Cancel => unreachable!(), // Handled above
                }
                
                // Return to timer view
                app.navigate_to(app::View::Timer);
            }
            Err(e) => {
                app.set_status(format!("Error saving timer: {}", e));
                app.navigate_to(app::View::Timer);
            }
        }
    } else {
        app.set_status("Error: No start time recorded".to_string());
        app.navigate_to(app::View::Timer);
    }
    
    Ok(())
}

// Helper functions for edit mode

/// Handle Enter key in edit mode - open modal for Project/Activity/Note or move to next field
fn handle_entry_edit_enter(app: &mut App) {
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
                    let note = state.note.clone();
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
                app.saved_timer_note = Some(app.description_input.clone());
                // Set description_input from the edit state before navigating
                if let Some(n) = note {
                    app.description_input = n;
                }
                // Open description editor
                app.navigate_to(app::View::EditDescription);
            }
            _ => {}
        }
    }
}

/// Save changes from This Week edit mode to database
async fn handle_this_week_edit_save(app: &mut App, db: &api::Database) -> Result<()> {
    if let Some(state) = &app.this_week_edit_state {
        // Parse the time inputs
        let start_parts: Vec<&str> = state.start_time_input.split(':').collect();
        let end_parts: Vec<&str> = state.end_time_input.split(':').collect();

        if start_parts.len() != 2 || end_parts.len() != 2 {
            app.set_status("Error: Invalid time format".to_string());
            app.exit_this_week_edit_mode();
            return Ok(());
        }

        let start_hours: u8 = start_parts[0].parse().unwrap_or(0);
        let start_mins: u8 = start_parts[1].parse().unwrap_or(0);
        let end_hours: u8 = end_parts[0].parse().unwrap_or(0);
        let end_mins: u8 = end_parts[1].parse().unwrap_or(0);

        // Get the entry being edited to preserve the date
        let entries = app.this_week_history();
        let entry_date = entries
            .iter()
            .find(|e| e.id == state.entry_id)
            .map(|e| e.start_time.date())
            .unwrap_or_else(|| time::OffsetDateTime::now_utc().date());

        // Construct new times (using entry's date, treating input as local time)
        let local_offset = time::UtcOffset::current_local_offset()
            .unwrap_or(time::UtcOffset::UTC);
        
        let start_time = time::OffsetDateTime::new_in_offset(
            entry_date,
            time::Time::from_hms(start_hours, start_mins, 0).unwrap(),
            local_offset,
        );
        
        let end_time = time::OffsetDateTime::new_in_offset(
            entry_date,
            time::Time::from_hms(end_hours, end_mins, 0).unwrap(),
            local_offset,
        );

        // Update via API
        match db.update_timer_entry(
            state.entry_id,
            start_time,
            end_time,
            state.project_id.clone(),
            state.project_name.clone(),
            state.activity_id.clone(),
            state.activity_name.clone(),
            Some(state.note.clone()),
        ).await {
            Ok(_) => {
                app.set_status("Entry updated successfully".to_string());
                // Refresh history
                match db.get_timer_history(app.user_id, 500).await {
                    Ok(history) => {
                        app.update_history(history);
                        app.rebuild_history_list();
                    }
                    Err(e) => {
                        app.set_status(format!("Warning: Could not refresh history: {}", e));
                    }
                }
            }
            Err(e) => {
                app.set_status(format!("Error updating entry: {}", e));
            }
        }
    }

    app.exit_this_week_edit_mode();
    Ok(())
}

/// Save changes from History edit mode to database
async fn handle_history_edit_save(app: &mut App, db: &api::Database) -> Result<()> {
    if let Some(state) = &app.history_edit_state {
        // Parse the time inputs
        let start_parts: Vec<&str> = state.start_time_input.split(':').collect();
        let end_parts: Vec<&str> = state.end_time_input.split(':').collect();

        if start_parts.len() != 2 || end_parts.len() != 2 {
            app.set_status("Error: Invalid time format".to_string());
            app.exit_history_edit_mode();
            return Ok(());
        }

        let start_hours: u8 = start_parts[0].parse().unwrap_or(0);
        let start_mins: u8 = start_parts[1].parse().unwrap_or(0);
        let end_hours: u8 = end_parts[0].parse().unwrap_or(0);
        let end_mins: u8 = end_parts[1].parse().unwrap_or(0);

        // Get the entry being edited to preserve the date
        let entry_date = app
            .timer_history
            .iter()
            .find(|e| e.id == state.entry_id)
            .map(|e| e.start_time.date())
            .unwrap_or_else(|| time::OffsetDateTime::now_utc().date());

        // Construct new times (using entry's date, treating input as local time)
        let local_offset = time::UtcOffset::current_local_offset()
            .unwrap_or(time::UtcOffset::UTC);
        
        let start_time = time::OffsetDateTime::new_in_offset(
            entry_date,
            time::Time::from_hms(start_hours, start_mins, 0).unwrap(),
            local_offset,
        );
        
        let end_time = time::OffsetDateTime::new_in_offset(
            entry_date,
            time::Time::from_hms(end_hours, end_mins, 0).unwrap(),
            local_offset,
        );

        // Update via API
        match db.update_timer_entry(
            state.entry_id,
            start_time,
            end_time,
            state.project_id.clone(),
            state.project_name.clone(),
            state.activity_id.clone(),
            state.activity_name.clone(),
            Some(state.note.clone()),
        ).await {
            Ok(_) => {
                app.set_status("Entry updated successfully".to_string());
                // Refresh history
                match db.get_timer_history(app.user_id, 500).await {
                    Ok(history) => {
                        app.update_history(history);
                        app.rebuild_history_list();
                    }
                    Err(e) => {
                        app.set_status(format!("Warning: Could not refresh history: {}", e));
                    }
                }
            }
            Err(e) => {
                app.set_status(format!("Error updating entry: {}", e));
            }
        }
    }

    app.exit_history_edit_mode();
    Ok(())
}

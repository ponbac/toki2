mod api;
mod app;
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

    // Load timer history
    match db.get_timer_history(user_id, 50).await {
        Ok(history) => {
            app.update_history(history);
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
                        let had_edit_state = app.today_edit_state.is_some();
                        let saved_edit_state = app.today_edit_state.clone();
                        // Save running timer's project/activity
                        let saved_selected_project = app.selected_project.clone();
                        let saved_selected_activity = app.selected_activity.clone();
                        
                        match key.code {
                            KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                // Ctrl+X clears search input
                                app.search_input_clear();
                            }
                            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) && c != 'q' && c != 'Q' && c != 'j' && c != 'k' => {
                                // Type to search (except for navigation/quit keys)
                                app.search_input_char(c);
                            }
                            KeyCode::Backspace => {
                                app.search_input_backspace();
                            }
                            KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
                            KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                            KeyCode::Enter => {
                                app.confirm_selection();
                                // If we were in edit mode, restore with selected project AND restore running timer state
                                if had_edit_state {
                                    if let Some(mut edit_state) = saved_edit_state {
                                        if let Some(project) = app.selected_project.clone() {
                                            edit_state.project_id = Some(project.id.clone());
                                            edit_state.project_name = Some(project.name.clone());
                                        }
                                        app.today_edit_state = Some(edit_state);
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
                        let was_in_edit_mode = app.today_edit_state.is_some();
                        let saved_edit_state = app.today_edit_state.clone();
                        let saved_selected_project = app.selected_project.clone();
                        let saved_selected_activity = app.selected_activity.clone();
                        
                        match key.code {
                            KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                // Ctrl+X clears search input
                                app.activity_search_input_clear();
                            }
                            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) && c != 'q' && c != 'Q' && c != 'j' && c != 'k' => {
                                // Type to search (except for navigation/quit keys)
                                app.activity_search_input_char(c);
                            }
                            KeyCode::Backspace => {
                                app.activity_search_input_backspace();
                            }
                            KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
                            KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                            KeyCode::Enter => {
                                app.confirm_selection();
                                
                                // If we were in edit mode, restore edit state with selected activity AND restore running timer state
                                if was_in_edit_mode {
                                    if let Some(mut edit_state) = saved_edit_state {
                                        if let Some(activity) = app.selected_activity.clone() {
                                            edit_state.activity_id = Some(activity.id.clone());
                                            edit_state.activity_name = Some(activity.name.clone());
                                        }
                                        app.today_edit_state = Some(edit_state);
                                    }
                                    // Restore running timer's project/activity
                                    app.selected_project = saved_selected_project;
                                    app.selected_activity = saved_selected_activity;
                                    // Navigate to Timer but stay in Today edit mode with Activity field focused
                                    app.navigate_to(app::View::Timer);
                                    app.focused_box = app::FocusedBox::Today;
                                    app.today_edit_set_focused_field(app::TodayEditField::Activity);
                                }
                            }
                            KeyCode::Esc => app.cancel_selection(),
                            KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
                            _ => {}
                        }
                    }
                    app::View::EditDescription => {
                        let was_in_edit_mode = app.today_edit_state.is_some();
                        
                        match key.code {
                            KeyCode::Char('x') | KeyCode::Char('X') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                app.description_input.clear();
                            }
                            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                                app.input_char(c);
                            }
                            KeyCode::Backspace => app.input_backspace(),
                            KeyCode::Enter => {
                                // If in edit mode, save to edit state
                                if was_in_edit_mode {
                                    if let Some(mut edit_state) = app.today_edit_state.take() {
                                        edit_state.note = app.description_input.clone();
                                        app.today_edit_state = Some(edit_state);
                                    }
                                    // Restore running timer's note
                                    if let Some(saved_note) = app.saved_timer_note.take() {
                                        app.description_input = saved_note;
                                    }
                                    // Navigate to Timer but keep focused on Today box with edit state
                                    app.navigate_to(app::View::Timer);
                                    app.focused_box = app::FocusedBox::Today;
                                } else {
                                    app.confirm_description();
                                }
                            }
                            KeyCode::Esc => {
                                // If in edit mode, cancel without saving
                                if was_in_edit_mode {
                                    // Restore running timer's note
                                    if let Some(saved_note) = app.saved_timer_note.take() {
                                        app.description_input = saved_note;
                                    }
                                    app.navigate_to(app::View::Timer);
                                    app.focused_box = app::FocusedBox::Today;
                                } else {
                                    app.cancel_selection();
                                }
                            }
                            KeyCode::Char('q') | KeyCode::Char('Q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                app.quit();
                            }
                            _ => {}
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
                        match key.code {
                            KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
                            KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                            KeyCode::Char('h') | KeyCode::Char('H') | KeyCode::Esc => {
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
                                if app.today_edit_state.is_some() {
                                    app.today_edit_next_field();
                                } else {
                                    app.focus_next();
                                }
                            }
                            // Shift+Tab (BackTab): Navigate backward between boxes (or prev field in edit mode)
                            KeyCode::BackTab => {
                                if app.today_edit_state.is_some() {
                                    app.today_edit_prev_field();
                                } else {
                                    app.focus_previous();
                                }
                            }
                            // Arrow down / j: Move down (next row in Today, or next field in edit mode)
                            KeyCode::Down | KeyCode::Char('j') => {
                                if app.today_edit_state.is_some() {
                                    app.today_edit_next_field();
                                } else if app.focused_box == app::FocusedBox::Today {
                                    app.today_focus_down();
                                } else {
                                    app.focus_next();
                                }
                            }
                            // Arrow up / k: Move up (prev row in Today, or prev field in edit mode)
                            KeyCode::Up | KeyCode::Char('k') => {
                                if app.today_edit_state.is_some() {
                                    app.today_edit_prev_field();
                                } else if app.focused_box == app::FocusedBox::Today {
                                    app.today_focus_up();
                                } else {
                                    app.focus_previous();
                                }
                            }
                            // Arrow right / l: Next field (edit mode only)
                            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('L') => {
                                if app.today_edit_state.is_some() {
                                    app.today_edit_next_field();
                                }
                            }
                            // Arrow left / h: Prev field (edit mode only)
                            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('H') => {
                                if app.today_edit_state.is_some() {
                                    app.today_edit_prev_field();
                                } else if app.today_edit_state.is_none() {
                                    // Open History view when not in edit mode
                                    match db.get_timer_history(app.user_id, 50).await {
                                        Ok(history) => {
                                            app.update_history(history);
                                            app.navigate_to(app::View::History);
                                        }
                                        Err(e) => {
                                            app.set_status(format!("Error loading history: {}", e));
                                        }
                                    }
                                }
                            }
                            // Vim-style navigation between boxes (lowercase j/k without modifiers)
                            // Enter: activate focused box or start timer if Timer box selected
                            KeyCode::Enter => {
                                if app.today_edit_state.is_some() {
                                    // In edit mode, Enter on Project/Activity/Note opens modal
                                    handle_today_edit_enter(app);
                                } else {
                                    match app.focused_box {
                                        app::FocusedBox::Timer => {
                                            // Start timer when Timer box is focused
                                            handle_start_timer(app)?;
                                        }
                                        app::FocusedBox::Today => {
                                            // If no entry selected, default to first entry
                                            if app.focused_today_index.is_none() && !app.todays_history().is_empty() {
                                                app.focused_today_index = Some(0);
                                            }
                                            app.enter_today_edit_mode();
                                        }
                                        _ => {
                                            app.activate_focused_box();
                                        }
                                    }
                                }
                            }
                            // Number keys for time input in edit mode
                            KeyCode::Char(c) if app.today_edit_state.is_some() && c.is_ascii_digit() => {
                                app.today_edit_input_char(c);
                            }
                            KeyCode::Backspace => {
                                if app.today_edit_state.is_some() {
                                    app.today_edit_backspace();
                                }
                            }
                            // Escape to exit edit mode
                            KeyCode::Esc => {
                                if app.today_edit_state.is_some() {
                                    // Validate and save
                                    if let Some(error) = app.today_edit_validate() {
                                        // Show error, don't exit
                                        if let Some(state) = &mut app.today_edit_state {
                                            state.validation_error = Some(error);
                                        }
                                    } else {
                                        // Save changes via API
                                        handle_today_edit_save(app, db).await?;
                                    }
                                } else {
                                    app.focused_box = app::FocusedBox::Timer;
                                    app.focused_today_index = None;
                                }
                            }
                            // Space: Start timer (backwards compat)
                            KeyCode::Char(' ') => {
                                handle_start_timer(app)?;
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
                            // Ctrl+X: Clear timer and reset to default state
                            KeyCode::Char('x') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                app.clear_timer();
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
                    app::SaveAction::SaveAndPause => {
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

// Helper functions for Today edit mode

/// Handle Enter key in Today edit mode - open modal for Project/Activity/Note or move to next field
fn handle_today_edit_enter(app: &mut App) {
    if let Some(state) = &app.today_edit_state {
        match state.focused_field {
            app::TodayEditField::Project => {
                // Save current edit state, open project modal
                app.navigate_to(app::View::SelectProject);
            }
            app::TodayEditField::Activity => {
                // For activity, we need a project first
                if state.project_id.is_some() {
                    app.navigate_to(app::View::SelectActivity);
                } else {
                    app.set_status("Please select a project first".to_string());
                }
            }
            app::TodayEditField::Note => {
                // Save running timer's note before overwriting with entry's note
                app.saved_timer_note = Some(app.description_input.clone());
                // Set description_input from the edit state before navigating
                app.description_input = state.note.clone();
                // Open description editor
                app.navigate_to(app::View::EditDescription);
            }
            app::TodayEditField::StartTime | app::TodayEditField::EndTime => {
                // Move to next field (like Tab)
                app.today_edit_next_field();
            }
        }
    }
}

/// Save changes from Today edit mode to database
async fn handle_today_edit_save(app: &mut App, db: &api::Database) -> Result<()> {
    if let Some(state) = &app.today_edit_state {
        // Parse the time inputs
        let start_parts: Vec<&str> = state.start_time_input.split(':').collect();
        let end_parts: Vec<&str> = state.end_time_input.split(':').collect();

        if start_parts.len() != 2 || end_parts.len() != 2 {
            app.set_status("Error: Invalid time format".to_string());
            app.exit_today_edit_mode();
            return Ok(());
        }

        let start_hours: u8 = start_parts[0].parse().unwrap_or(0);
        let start_mins: u8 = start_parts[1].parse().unwrap_or(0);
        let end_hours: u8 = end_parts[0].parse().unwrap_or(0);
        let end_mins: u8 = end_parts[1].parse().unwrap_or(0);

        // Get the entry being edited to preserve the date
        let todays = app.todays_history();
        let entry_date = todays
            .iter()
            .find(|e| e.id == state.entry_id)
            .map(|e| e.start_time.date())
            .unwrap_or_else(|| time::OffsetDateTime::now_utc().date());

        // Construct new times (using today's date, treating input as local time)
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
                match db.get_timer_history(app.user_id, 50).await {
                    Ok(history) => {
                        app.update_history(history);
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

    app.exit_today_edit_mode();
    Ok(())
}

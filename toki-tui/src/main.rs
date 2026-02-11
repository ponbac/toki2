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
                        match key.code {
                            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                // Ctrl+U clears search input
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
                                // Auto-show activity selection
                                app.navigate_to(app::View::SelectActivity);
                            }
                            KeyCode::Esc => app.cancel_selection(),
                            KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
                            _ => {}
                        }
                    }
                    app::View::SelectActivity => {
                        match key.code {
                            KeyCode::Up | KeyCode::Char('k') => app.select_previous(),
                            KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                            KeyCode::Enter => {
                                app.confirm_selection();
                                // Just update local state - no database operations
                            }
                            KeyCode::Esc => app.cancel_selection(),
                            KeyCode::Char('q') | KeyCode::Char('Q') => app.quit(),
                            _ => {}
                        }
                    }
                    app::View::EditDescription => {
                        match key.code {
                            KeyCode::Char('x') | KeyCode::Char('X') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                // Clear description with Ctrl+X
                                app.description_input.clear();
                            }
                            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                                app.input_char(c);
                            }
                            KeyCode::Backspace => app.input_backspace(),
                            KeyCode::Enter => {
                                // Just update local state - no database operations
                                app.confirm_description();
                            }
                            KeyCode::Esc => app.cancel_selection(),
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
                            // Tab: Navigate between boxes
                            KeyCode::Tab => {
                                app.focus_next();
                            }
                            // Arrow keys: Navigate between boxes
                            KeyCode::Down => {
                                app.focus_next();
                            }
                            KeyCode::Up => {
                                app.focus_previous();
                            }
                            // Vim-style navigation between boxes (lowercase j/k without modifiers)
                            KeyCode::Char('j') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                                app.focus_next();
                            }
                            KeyCode::Char('k') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                                app.focus_previous();
                            }
                            // Enter: activate focused box or start timer if Timer box selected
                            KeyCode::Enter => {
                                match app.focused_box {
                                    app::FocusedBox::Timer => {
                                        // Start timer when Timer box is focused
                                        handle_start_timer(app)?;
                                    }
                                    _ => {
                                        app.activate_focused_box();
                                    }
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
                            // A: Edit annotation/description
                            KeyCode::Char('a') | KeyCode::Char('A') => {
                                app.navigate_to(app::View::EditDescription);
                            }
                            // H: View history
                            KeyCode::Char('h') | KeyCode::Char('H') => {
                                // Refresh history before showing
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

mod api_client;
mod app;
mod config;
mod git;
mod login;
mod types;
mod ui;

use anyhow::{Context, Result};
use api_client::ApiClient;
use app::{App, TextInput};
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
    let args: Vec<String> = std::env::args().collect();
    let flag = args.get(1).map(|s| s.as_str());

    if matches!(flag, Some("--config-path")) {
        let path = config::TokiConfig::config_path()?;
        if !path.exists() {
            config::TokiConfig::default().save()?;
            println!("Created default config at {}", path.display());
        } else {
            println!("{}", path.display());
        }
        return Ok(());
    }

    let cfg = config::TokiConfig::load()?;

    match flag {
        Some("--login") => {
            login::run_login(&cfg.api_url).await?;
            return Ok(());
        }
        Some("--logout") => {
            config::TokiConfig::clear_session()?;
            config::TokiConfig::clear_mt_cookies()?;
            println!("Logged out. Session and Milltime cookies cleared.");
            return Ok(());
        }
        Some("--dev") => {
            let mut client = ApiClient::dev()?;
            let me = client.me().await?;
            println!("Dev mode: logged in as {} ({})\n", me.full_name, me.email);
            let mut app = App::new(me.id, &cfg);
            {
                let today = time::OffsetDateTime::now_utc().date();
                let month_ago = today - time::Duration::days(30);
                if let Ok(entries) = client.get_time_entries(month_ago, today).await {
                    app.update_history(entries);
                    app.rebuild_history_list();
                }
            }
            if let Ok(projects) = client.get_projects().await {
                app.set_projects_activities(projects, vec![]);
            }
            if let Ok(Some(timer)) = client.get_active_timer().await {
                restore_active_timer(&mut app, timer);
            }
            enable_raw_mode()?;
            let mut stdout = io::stdout();
            execute!(stdout, EnterAlternateScreen)?;
            let backend = CrosstermBackend::new(stdout);
            let mut terminal = Terminal::new(backend)?;
            let res = run_app(&mut terminal, &mut app, &mut client).await;
            disable_raw_mode()?;
            execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
            terminal.show_cursor()?;
            if let Err(err) = res {
                eprintln!("Error: {:?}", err);
            }
            println!("\nGoodbye!");
            return Ok(());
        }
        _ => {}
    }

    // Load session — require login if missing
    let session_id = match config::TokiConfig::load_session()? {
        Some(s) => s,
        None => {
            eprintln!("Not logged in. Run `toki-tui --login` to authenticate.");
            std::process::exit(1);
        }
    };

    let mt_cookies = config::TokiConfig::load_mt_cookies()?;
    let mut client = ApiClient::new(&cfg.api_url, &session_id, mt_cookies)?;

    // Verify session is valid
    let me = match client.me().await {
        Ok(me) => me,
        Err(e) => {
            eprintln!("{}", e);
            std::process::exit(1);
        }
    };

    println!("Logged in as {} ({})\n", me.full_name, me.email);

    // Authenticate against Milltime if we don't have cookies yet
    if client.mt_cookies().is_empty() {
        println!("Milltime credentials required.");
        print!("Username: ");
        std::io::Write::flush(&mut std::io::stdout())?;
        let mut username = String::new();
        std::io::BufRead::read_line(&mut std::io::BufReader::new(std::io::stdin()), &mut username)?;
        let username = username.trim().to_string();

        let password = rpassword::prompt_password("Password: ")?;

        print!("Authenticating...");
        std::io::Write::flush(&mut std::io::stdout())?;
        match client.authenticate(&username, &password).await {
            Ok(cookies) => {
                client.update_mt_cookies(cookies);
                config::TokiConfig::save_mt_cookies(client.mt_cookies())?;
                println!(" OK");
            }
            Err(e) => {
                eprintln!("\nMilltime authentication failed: {}", e);
                std::process::exit(1);
            }
        }
    }

    let mut app = App::new(me.id, &cfg);

    // Load timer history (last 30 days from Milltime)
    app.is_loading = true;
    {
        let today = time::OffsetDateTime::now_utc().date();
        let month_ago = today - time::Duration::days(30);
        match client.get_time_entries(month_ago, today).await {
            Ok(entries) => {
                app.update_history(entries);
                app.rebuild_history_list();
            }
            Err(e) => eprintln!("Warning: Could not load history: {}", e),
        }
    }

    // Fetch projects from Milltime API
    match client.get_projects().await {
        Ok(projects) => {
            app.set_projects_activities(projects, vec![]);
        }
        Err(e) => eprintln!("Warning: Could not load projects: {}", e),
    }

    // Restore running timer from server (if one was left running)
    match client.get_active_timer().await {
        Ok(Some(timer)) => {
            restore_active_timer(&mut app, timer);
            println!("Restored running timer from server.");
        }
        Ok(None) => {}
        Err(e) => eprintln!("Warning: Could not check active timer: {}", e),
    }

    // Compute Mon–Sun of the current ISO week for time-info query
    let today = time::OffsetDateTime::now_utc()
        .to_offset(time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC))
        .date();
    let days_from_monday = today.weekday().number_days_from_monday() as i64;
    let week_start = today - time::Duration::days(days_from_monday);
    let week_end = week_start + time::Duration::days(6);

    // Fetch scheduled hours per week from Milltime
    match client.get_time_info(week_start, week_end).await {
        Ok(time_info) => {
            app.scheduled_hours_per_week = time_info.scheduled_period_time;
            app.flex_time_current = time_info.flex_time_current;
        }
        Err(e) => eprintln!("Warning: Could not load time info: {}", e),
    }

    app.is_loading = false;

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Run the app
    let res = run_app(&mut terminal, &mut app, &mut client).await;

    // Restore terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        eprintln!("Error: {:?}", err);
    }

    println!("\nGoodbye!");

    Ok(())
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
    client: &mut ApiClient,
) -> Result<()> {
    // Show throbber for at least 3 seconds on startup
    app.is_loading = true;
    let loading_until = std::time::Instant::now() + std::time::Duration::from_secs(3);

    // Background polling: refresh time entries every 60 seconds
    let mut last_history_refresh = std::time::Instant::now();
    const HISTORY_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(60);

    loop {
        terminal.draw(|f| ui::render(f, app))?;

        // Advance throbber every frame when loading
        if app.is_loading {
            app.throbber_state.calc_next();
            // Stop the startup animation after 3 seconds (real loads set is_loading themselves)
            if std::time::Instant::now() >= loading_until {
                app.is_loading = false;
            }
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                // Milltime re-auth overlay intercepts all keys while it is open
                if app.milltime_reauth.is_some() {
                    match key.code {
                        KeyCode::Tab | KeyCode::BackTab => {
                            app.milltime_reauth_next_field();
                        }
                        KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                            app.milltime_reauth_input_char(c);
                        }
                        KeyCode::Backspace => {
                            app.milltime_reauth_backspace();
                        }
                        KeyCode::Enter => {
                            handle_milltime_reauth_submit(app, client).await;
                        }
                        KeyCode::Esc => {
                            app.close_milltime_reauth();
                            app.set_status("Milltime re-authentication cancelled".to_string());
                        }
                        _ => {}
                    }
                    continue;
                }

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
                            KeyCode::Left  => { if !app.selection_list_focused { app.search_move_cursor(true); } }
                            KeyCode::Right => { if !app.selection_list_focused { app.search_move_cursor(false); } }
                            KeyCode::Home  => { if !app.selection_list_focused { app.search_cursor_home_end(true); } }
                            KeyCode::End   => { if !app.selection_list_focused { app.search_cursor_home_end(false); } }
                            KeyCode::Enter => {
                                app.confirm_selection();
                                // Fetch activities for the selected project (lazy, cached)
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
                                    // Populate app.activities from cache for this project
                                    if let Some(cached) = app.activity_cache.get(&project.id) {
                                        app.activities = cached.clone();
                                        app.filtered_activities = cached.clone();
                                        app.filtered_activity_index = 0;
                                    }
                                }
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
                            KeyCode::Left  => { if !app.selection_list_focused { app.activity_search_move_cursor(true); } }
                            KeyCode::Right => { if !app.selection_list_focused { app.activity_search_move_cursor(false); } }
                            KeyCode::Home  => { if !app.selection_list_focused { app.activity_search_cursor_home_end(true); } }
                            KeyCode::End   => { if !app.selection_list_focused { app.activity_search_cursor_home_end(false); } }
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
                                } else if app.timer_state == app::TimerState::Running {
                                    // Sync new project/activity to server
                                    let project_id = app.selected_project.as_ref().map(|p| p.id.clone());
                                    let project_name = app.selected_project.as_ref().map(|p| p.name.clone());
                                    let activity_id = app.selected_activity.as_ref().map(|a| a.id.clone());
                                    let activity_name = app.selected_activity.as_ref().map(|a| a.name.clone());
                                    if let Err(e) = client.update_active_timer(
                                        project_id, project_name, activity_id, activity_name,
                                        None, None,
                                    ).await {
                                        app.set_status(format!("Warning: Could not sync project to server: {}", e));
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
                                KeyCode::Left  => app.cwd_move_cursor(true),
                                KeyCode::Right => app.cwd_move_cursor(false),
                                KeyCode::Home  => app.cwd_cursor_home_end(true),
                                KeyCode::End   => app.cwd_cursor_home_end(false),
                                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    app.cwd_input_char(c);
                                }
                                _ => {}
                            }
                        } else if app.taskwarrior_overlay.is_some() {
                            match key.code {
                                KeyCode::Esc => app.close_taskwarrior_overlay(),
                                KeyCode::Char('t') | KeyCode::Char('T')
                                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    app.close_taskwarrior_overlay();
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    app.taskwarrior_move(true);
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    app.taskwarrior_move(false);
                                }
                                KeyCode::Enter => app.taskwarrior_confirm(),
                                _ => {}
                            }
                        } else if app.git_mode {
                            // Second key of Ctrl+G sequence
                            match key.code {
                                KeyCode::Char('b') | KeyCode::Char('B') => app.paste_git_branch_raw(),
                                KeyCode::Char('p') | KeyCode::Char('P') => app.paste_git_branch_parsed(),
                                KeyCode::Char('c') | KeyCode::Char('C') => app.paste_git_last_commit(),
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
                                KeyCode::Char('t') | KeyCode::Char('T')
                                    if key.modifiers.contains(KeyModifiers::CONTROL) =>
                                {
                                    app.open_taskwarrior_overlay();
                                }
                                KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                                    app.input_char(c);
                                }
                                KeyCode::Backspace => app.input_backspace(),
                                KeyCode::Left  => app.input_move_cursor(true),
                                KeyCode::Right => app.input_move_cursor(false),
                                KeyCode::Home  => app.input_cursor_home_end(true),
                                KeyCode::End   => app.input_cursor_home_end(false),
                                KeyCode::Enter => {
                                    if was_in_edit_mode {
                                        app.update_edit_state_note(app.description_input.value.clone());
                                        if let Some(saved_note) = app.saved_timer_note.take() {
                                            app.description_input = TextInput::from_str(&saved_note);
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
                                            app.description_input = TextInput::from_str(&saved_note);
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
                                handle_save_timer_with_action(app, client).await?;
                            }
                            KeyCode::Char('2') => {
                                app.select_save_action_by_number(2);
                                handle_save_timer_with_action(app, client).await?;
                            }
                            KeyCode::Char('3') => {
                                app.select_save_action_by_number(3);
                                handle_save_timer_with_action(app, client).await?;
                            }
                            KeyCode::Char('4') | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => {
                                // Cancel - return to timer view
                                app.navigate_to(app::View::Timer);
                            }
                            KeyCode::Up | KeyCode::Char('k') => app.select_previous_save_action(),
                            KeyCode::Down | KeyCode::Char('j') => app.select_next_save_action(),
                            KeyCode::Enter => {
                                handle_save_timer_with_action(app, client).await?;
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
                                 // Arrow keys: navigate fields (or cursor movement in Note)
                                 KeyCode::Down | KeyCode::Char('j') => {
                                     app.entry_edit_next_field();
                                 }
                                 KeyCode::Up | KeyCode::Char('k') => {
                                     app.entry_edit_prev_field();
                                 }
                                 KeyCode::Right => {
                                     if app.history_edit_state.as_ref().is_some_and(|s| s.focused_field == app::EntryEditField::Note) {
                                         app.entry_edit_move_cursor(false);
                                     } else {
                                         app.entry_edit_next_field();
                                     }
                                 }
                                 KeyCode::Char('l') | KeyCode::Char('L') => {
                                     app.entry_edit_next_field();
                                 }
                                 KeyCode::Left => {
                                     if app.history_edit_state.as_ref().is_some_and(|s| s.focused_field == app::EntryEditField::Note) {
                                         app.entry_edit_move_cursor(true);
                                     } else {
                                         app.entry_edit_prev_field();
                                     }
                                 }
                                 KeyCode::Char('h') | KeyCode::Char('H') => {
                                     app.entry_edit_prev_field();
                                 }
                                 KeyCode::Home => app.entry_edit_cursor_home_end(true),
                                 KeyCode::End  => app.entry_edit_cursor_home_end(false),
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
                                        handle_history_edit_save(app, client).await?;
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
                                KeyCode::Delete | KeyCode::Backspace
                                    if app.focused_history_index.is_some() =>
                                {
                                    app.enter_delete_confirm(app::DeleteOrigin::History);
                                }
                                KeyCode::Char('x')
                                    if key.modifiers.contains(KeyModifiers::CONTROL)
                                        && app.focused_history_index.is_some() =>
                                {
                                    app.enter_delete_confirm(app::DeleteOrigin::History);
                                }
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
                    app::View::ConfirmDelete => {
                        match key.code {
                            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                                if let Some(ctx) = app.delete_context.take() {
                                    let origin = ctx.origin;
                                    match client.delete_time_entry(&ctx.registration_id).await {
                                        Ok(()) => {
                                            // Remove from local state immediately
                                            app.time_entries.retain(|e| e.registration_id != ctx.registration_id);
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
                            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                                let origin = app.delete_context.as_ref().map(|c| c.origin);
                                app.delete_context = None;
                                match origin {
                                    Some(app::DeleteOrigin::Timer) | None => app.navigate_to(app::View::Timer),
                                    Some(app::DeleteOrigin::History) => app.navigate_to(app::View::History),
                                }
                            }
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
                             // Arrow right / l: Next field (edit mode only; Note is not inline-editable)
                             KeyCode::Right => {
                                 if app.this_week_edit_state.is_some() {
                                     app.entry_edit_next_field();
                                 }
                             }
                             KeyCode::Char('l') | KeyCode::Char('L') => {
                                 if app.this_week_edit_state.is_some() {
                                     app.entry_edit_next_field();
                                 }
                             }
                             // Arrow left: Prev field in edit mode (Note is not inline-editable)
                             KeyCode::Left => {
                                 if app.this_week_edit_state.is_some() {
                                     app.entry_edit_prev_field();
                                 }
                             }
                             KeyCode::Char('h') | KeyCode::Char('H') => {
                                 if app.this_week_edit_state.is_some() {
                                     app.entry_edit_prev_field();
                                 } else                          if app.this_week_edit_state.is_none() {
                                     // Open History view when not in edit mode
                                     let today = time::OffsetDateTime::now_utc().date();
                                     let month_ago = today - time::Duration::days(30);
                                     match client.get_time_entries(month_ago, today).await {
                                         Ok(entries) => {
                                             app.update_history(entries);
                                             app.rebuild_history_list();
                                             app.navigate_to(app::View::History);
                                         }
                                         Err(e) => {
                                             app.set_status(format!("Error loading history: {}", e));
                                         }
                                     }
                                 }
                             }
                             KeyCode::Home => {
                                 if app.this_week_edit_state.is_some() {
                                     app.entry_edit_cursor_home_end(true);
                                 }
                             }
                             KeyCode::End => {
                                 if app.this_week_edit_state.is_some() {
                                     app.entry_edit_cursor_home_end(false);
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
                                            handle_start_timer(app, client).await?;
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
                                    // Note field is not inline-editable; Enter opens the Notes view
                                    let on_note = app.this_week_edit_state.as_ref()
                                        .is_some_and(|s| s.focused_field == app::EntryEditField::Note);
                                    if !on_note {
                                        app.entry_edit_backspace();
                                    }
                                } else if app.focused_box == app::FocusedBox::Today
                                    && app.focused_this_week_index.is_some_and(|idx| {
                                        !(app.timer_state == app::TimerState::Running && idx == 0)
                                    })
                                {
                                    app.enter_delete_confirm(app::DeleteOrigin::Timer);
                                }
                            }
                            // Escape to exit zen mode first, then exit edit mode
                            KeyCode::Esc => {
                                if app.zen_mode {
                                    app.exit_zen_mode();
                                } else if app.this_week_edit_state.is_some() {
                                    // Check validation
                                    if let Some(error) = app.entry_edit_validate() {
                                        // Revert invalid times and show error
                                        app.entry_edit_revert_invalid_times();
                                        app.set_status(format!("Edit cancelled: {}", error));
                                        app.exit_this_week_edit_mode();
                                        app.focused_box = app::FocusedBox::Today;
                                    } else {
                                        // Save changes via API
                                        handle_this_week_edit_save(app, client).await?;
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
                                        handle_start_timer(app, client).await?;
                                    }
                                    app::TimerState::Running => {
                                        if !app.has_project_activity() {
                                            app.set_status("Cannot save: Please select Project / Activity first (press P)".to_string());
                                        } else {
                                            // Save & stop directly without showing dialog
                                            app.selected_save_action = app::SaveAction::SaveAndStop;
                                            handle_save_timer_with_action(app, client).await?;
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
                                    // If a DB entry row is selected, treat Ctrl+X as delete
                                    let selected_is_db_row = app.focused_box == app::FocusedBox::Today
                                        && app.focused_this_week_index.is_some_and(|idx| {
                                            !(app.timer_state == app::TimerState::Running && idx == 0)
                                        });
                                    if selected_is_db_row {
                                        app.enter_delete_confirm(app::DeleteOrigin::Timer);
                                    } else {
                                        // Original behaviour: discard running timer
                                        if app.timer_state == app::TimerState::Running {
                                            if let Err(e) = client.stop_timer().await {
                                                app.set_status(format!("Warning: Could not stop server timer: {}", e));
                                            }
                                        }
                                        app.clear_timer();
                                    }
                                }
                            }
                            KeyCode::Delete
                                if app.this_week_edit_state.is_none()
                                    && app.focused_box == app::FocusedBox::Today
                                    && app.focused_this_week_index.is_some_and(|idx| {
                                        !(app.timer_state == app::TimerState::Running && idx == 0)
                                    }) =>
                            {
                                app.enter_delete_confirm(app::DeleteOrigin::Timer);
                            }
                            // Z: Toggle zen mode
                            KeyCode::Char('z') | KeyCode::Char('Z') => app.toggle_zen_mode(),
                            _ => {}
                        }
                    }
                }
            }
        }

        // Background polling: silently refresh time entries every 60 seconds
        // Skip if user is in edit mode to avoid disrupting their input
        if last_history_refresh.elapsed() >= HISTORY_REFRESH_INTERVAL && !app.is_in_edit_mode() {
            let today = time::OffsetDateTime::now_utc().date();
            let month_ago = today - time::Duration::days(30);
            match client.get_time_entries(month_ago, today).await {
                Ok(entries) => {
                    app.update_history(entries);
                    app.rebuild_history_list();
                }
                Err(e) if is_milltime_auth_error(&e) => {
                    app.open_milltime_reauth();
                }
                Err(_) => {} // transient errors are silently ignored on background refresh
            }
            last_history_refresh = std::time::Instant::now();
        }

        if !app.running {
            break;
        }
    }

    Ok(())
}

/// Apply an active timer fetched from the server into App state.
fn restore_active_timer(app: &mut App, timer: crate::types::ActiveTimerState) {
    use std::time::{Duration, Instant};
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
            project_id: app.selected_project.as_ref().map(|p| p.id.clone()).unwrap_or_default(),
        });
    }
    if !timer.note.is_empty() {
        app.description_input = app::TextInput::from_str(&timer.note);
        app.description_is_default = false;
    }
}

async fn handle_start_timer(app: &mut App, client: &mut ApiClient) -> Result<()> {
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
            if let Err(e) = client.start_timer(project_id, project_name, activity_id, activity_name, note).await {
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

async fn handle_save_timer_with_action(app: &mut App, client: &mut ApiClient) -> Result<()> {
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
            {
                let today = time::OffsetDateTime::now_utc().date();
                let month_ago = today - time::Duration::days(30);
                if let Ok(entries) = client.get_time_entries(month_ago, today).await {
                    app.update_history(entries);
                    app.rebuild_history_list();
                }
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
                    if let Err(e) = client.start_timer(project_id, project_name, activity_id, activity_name, None).await {
                        app.set_status(format!("Saved but could not restart timer: {}", e));
                    } else {
                        app.start_timer();
                        app.set_status(format!("Saved {} to {} / {}", duration_str, project_display, activity_display));
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
                        app.set_status(format!("Saved {}. Timer started. Press P to select project.", duration_str));
                    }
                }
                app::SaveAction::SaveAndStop => {
                    app.timer_state = app::TimerState::Stopped;
                    app.absolute_start = None;
                    app.local_start = None;
                    if let Some(idx) = app.focused_this_week_index {
                        app.focused_this_week_index = if idx == 0 { None } else { Some(idx.saturating_sub(1)) };
                    }
                    app.set_status(format!("Saved {} to {} / {}", duration_str, project_display, activity_display));
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
async fn handle_this_week_edit_save(app: &mut App, client: &mut ApiClient) -> Result<()> {
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
    let local_offset = time::UtcOffset::current_local_offset()
        .unwrap_or(time::UtcOffset::UTC);
    let today = time::OffsetDateTime::now_utc().to_offset(local_offset).date();
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
    app.local_start = Some(std::time::Instant::now() - std::time::Duration::from_secs(elapsed_secs));

    app.selected_project = state.project_id.zip(state.project_name).map(|(id, name)| {
        types::Project { id, name }
    });
    app.selected_activity = state.activity_id.zip(state.activity_name).map(|(id, name)| {
        types::Activity { id, name, project_id: String::new() }
    });
    app.description_input = TextInput::from_str(&state.note.value);

    app.set_status("Running timer updated".to_string());

    // Sync updated start time / project / activity / note to server
    let project_id = app.selected_project.as_ref().map(|p| p.id.clone());
    let project_name = app.selected_project.as_ref().map(|p| p.name.clone());
    let activity_id = app.selected_activity.as_ref().map(|a| a.id.clone());
    let activity_name = app.selected_activity.as_ref().map(|a| a.name.clone());
    let note = if app.description_input.value.is_empty() { None } else { Some(app.description_input.value.clone()) };
    if let Err(e) = client.update_active_timer(
        project_id, project_name, activity_id, activity_name,
        note, app.absolute_start,
    ).await {
        app.set_status(format!("Warning: Could not sync timer to server: {}", e));
    }
}

/// Save changes from History edit mode to database
async fn handle_history_edit_save(app: &mut App, client: &mut ApiClient) -> Result<()> {
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
    let local_offset = time::UtcOffset::current_local_offset()
        .unwrap_or(time::UtcOffset::UTC);

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
    let original_activity_id =
        if state.activity_id.as_deref() != Some(entry.activity_id.as_str()) {
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
    {
        let today = time::OffsetDateTime::now_utc().date();
        let month_ago = today - time::Duration::days(30);
        match client.get_time_entries(month_ago, today).await {
            Ok(entries) => {
                app.update_history(entries);
                app.rebuild_history_list();
            }
            Err(e) => {
                app.set_status(format!(
                    "Entry updated (warning: could not reload history: {})",
                    e
                ));
                return Ok(());
            }
        }
    }

    app.set_status("Entry updated".to_string());
    Ok(())
}

/// Attempt Milltime re-authentication with the credentials from the overlay.
/// On success: updates cookies and closes the overlay.
/// On failure: sets the error message on the overlay so the user can retry.
async fn handle_milltime_reauth_submit(app: &mut App, client: &mut ApiClient) {
    let (username, password) = match app.milltime_reauth_credentials() {
        Some(creds) => creds,
        None => return,
    };
    if username.is_empty() {
        app.milltime_reauth_set_error("Username is required".to_string());
        return;
    }
    match client.authenticate(&username, &password).await {
        Ok(cookies) => {
            client.update_mt_cookies(cookies);
            if let Err(e) = config::TokiConfig::save_mt_cookies(client.mt_cookies()) {
                app.milltime_reauth_set_error(format!("Authenticated but could not save cookies: {}", e));
                return;
            }
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
fn is_milltime_auth_error(e: &anyhow::Error) -> bool {
    let msg = e.to_string().to_lowercase();
    msg.contains("unauthorized") || msg.contains("authenticate") || msg.contains("milltime")
}

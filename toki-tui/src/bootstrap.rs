use crate::api::ApiClient;
use crate::app::App;
use crate::runtime::restore_active_timer;

pub async fn initialize_app_state(app: &mut App, client: &mut ApiClient) {
    app.is_loading = true;

    let today = time::OffsetDateTime::now_utc().date();
    let month_ago = today - time::Duration::days(30);

    match client.get_time_entries(month_ago, today).await {
        Ok(entries) => {
            app.update_history(entries);
            app.rebuild_history_list();
        }
        Err(e) => eprintln!("Warning: Could not load history: {}", e),
    }

    match client.get_projects().await {
        Ok(projects) => {
            app.set_projects_activities(projects, vec![]);
        }
        Err(e) => eprintln!("Warning: Could not load projects: {}", e),
    }

    match client.get_active_timer().await {
        Ok(Some(timer)) => {
            restore_active_timer(app, timer);
            println!("Restored running timer from server.");
        }
        Ok(None) => {}
        Err(e) => eprintln!("Warning: Could not check active timer: {}", e),
    }

    let local_today = time::OffsetDateTime::now_utc()
        .to_offset(time::UtcOffset::current_local_offset().unwrap_or(time::UtcOffset::UTC))
        .date();
    let days_from_monday = local_today.weekday().number_days_from_monday() as i64;
    let week_start = local_today - time::Duration::days(days_from_monday);
    let week_end = week_start + time::Duration::days(6);

    match client.get_time_info(week_start, week_end).await {
        Ok(time_info) => {
            app.scheduled_hours_per_week = time_info.scheduled_period_time;
            app.flex_time_current = time_info.flex_time_current;
        }
        Err(e) => eprintln!("Warning: Could not load time info: {}", e),
    }

    app.is_loading = false;
}

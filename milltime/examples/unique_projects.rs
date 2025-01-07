use milltime::{Credentials, DateFilter, MilltimeClient};
use std::collections::HashMap;
use std::env;
use std::error::Error;

const START_DATE: &str = "2020-01-01";

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let credentials = get_credentials().await?;
    let client = MilltimeClient::new(credentials);

    // Get calendar from START_DATE to today
    let date_filter = format!("{},{}", START_DATE, chrono::Local::now().format("%Y-%m-%d"))
        .parse::<DateFilter>()?;
    let calendar = client.fetch_user_calendar(&date_filter).await?;

    // Create a map to store project info and total hours
    let mut project_times: HashMap<String, (String, f64)> = HashMap::new();

    // Sum up hours for each project
    calendar
        .weeks
        .into_iter()
        .flat_map(|week| week.days)
        .flat_map(|day| day.time_entries)
        .for_each(|entry| {
            project_times
                .entry(entry.project_id.clone())
                .and_modify(|(_, hours)| *hours += entry.hours)
                .or_insert((entry.project_name, entry.hours));
        });

    // Sort projects by total time (descending)
    let mut projects: Vec<_> = project_times.into_iter().collect();
    projects.sort_by(|a, b| b.1 .1.partial_cmp(&a.1 .1).unwrap());

    println!("Unique projects since {}:", START_DATE);
    for (_, (project_name, hours)) in projects {
        println!("{} ({})", project_name, format_hours_minutes(hours));
    }

    Ok(())
}

async fn get_credentials() -> Result<Credentials, Box<dyn Error>> {
    dotenvy::from_filename("./milltime/.env.local").ok();
    let username = env::var("MILLTIME_USERNAME").expect("MILLTIME_USERNAME must be set");
    let password = env::var("MILLTIME_PASSWORD").expect("MILLTIME_PASSWORD must be set");

    Credentials::new(&username, &password).await
}

fn format_hours_minutes(hours: f64) -> String {
    let total_minutes = (hours * 60.0).round() as i32;
    let hours = total_minutes / 60;
    let minutes = total_minutes % 60;
    format!("{:02}:{:02}", hours, minutes)
}

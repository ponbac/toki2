use milltime::{Credentials, DateFilter, MilltimeClient};
use std::collections::HashMap;
use std::error::Error;
use std::{cmp, env};

const START_DATE: &str = "2020-01-01";

struct ProjectInfo {
    project_name: String,
    hours: f64,
    first_entry: chrono::NaiveDate,
    last_entry: chrono::NaiveDate,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let credentials = get_credentials().await?;
    let client = MilltimeClient::new(credentials);

    // Get calendar from START_DATE to today
    let date_filter = format!("{},{}", START_DATE, chrono::Local::now().format("%Y-%m-%d"))
        .parse::<DateFilter>()?;
    let calendar = client.fetch_user_calendar(&date_filter).await?;

    // Create a map to store project info and total hours
    let mut project_times: HashMap<String, ProjectInfo> = HashMap::new();

    // Sum up hours for each project
    calendar
        .weeks
        .into_iter()
        .flat_map(|week| week.days)
        .flat_map(|day| day.time_entries)
        .for_each(|entry| {
            project_times
                .entry(entry.project_id.clone())
                .and_modify(|project_info| {
                    project_info.hours += entry.hours;
                    project_info.first_entry = cmp::min(project_info.first_entry, entry.date);
                    project_info.last_entry = cmp::max(project_info.last_entry, entry.date);
                })
                .or_insert(ProjectInfo {
                    project_name: entry.project_name,
                    hours: entry.hours,
                    first_entry: entry.date,
                    last_entry: entry.date,
                });
        });

    // Sort projects by total time (descending)
    let mut projects: Vec<_> = project_times.into_iter().collect();
    projects.sort_by(|a, b| {
        b.1.hours
            .partial_cmp(&a.1.hours)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    println!("Unique projects since {}:", START_DATE);
    for (
        _,
        ProjectInfo {
            project_name,
            hours,
            first_entry,
            last_entry,
        },
    ) in projects
    {
        println!(
            "{} ({}) | [{} - {}]",
            project_name,
            format_hours_minutes(hours),
            first_entry,
            last_entry
        );
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
    format!("{hours:02}:{minutes:02}")
}

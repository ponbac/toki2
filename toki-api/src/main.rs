use std::{collections::HashMap, env};

use az_devops::RepoClient;
use itertools::Itertools;
use plotly::{common::Title, layout::Axis, Layout, Plot, Scatter};
use time::Month;

#[tokio::main]
async fn main() {
    dotenvy::from_filename("./toki-api/.env.local").ok();

    let organization = env::var("ADO_ORGANIZATION").unwrap();
    let project = env::var("ADO_PROJECT").unwrap();
    let repo_name = env::var("ADO_REPO").unwrap();
    let token = env::var("ADO_TOKEN").unwrap();

    let repo_client = RepoClient::new(&repo_name, &organization, &project, &token)
        .await
        .unwrap();

    let pull_requests = repo_client.get_all_pull_requests().await.unwrap();
    // let pull_requests = repo_client.get_open_pull_requests().await.unwrap();

    for (i, pr) in pull_requests.iter().rev().enumerate() {
        println!(
            "{}: {} by {} ({})",
            i,
            pr.title,
            pr.created_by.display_name,
            pr.created_at.date(),
        );
    }

    let mut month_author_counts = HashMap::new();
    for pr in &pull_requests {
        let month = format!(
            "{}-{}",
            pr.created_at.year(),
            pr.created_at.month().as_double_digit_str()
        );
        let entry = month_author_counts
            .entry((month, pr.created_by.display_name.clone()))
            .or_insert(0);
        *entry += 1;
    }

    // Ensure each author has an entry for each month
    let mut author_months = HashMap::new();
    for ((month, author), count) in month_author_counts {
        author_months
            .entry(author)
            .or_insert_with(Vec::new)
            .push((month, count));
    }

    let mut plot = Plot::new();
    for (author, mut months_counts) in author_months {
        // Sort by month
        months_counts.sort_by(|(month_a, _), (month_b, _)| month_a.cmp(month_b));

        // Unzip into separate vectors
        let (dates, counts): (Vec<_>, Vec<_>) = months_counts.into_iter().unzip();
        let trace = Scatter::new(dates, counts).name(&author);
        plot.add_trace(trace);
    }

    let layout = Layout::new()
        .x_axis(Axis::new().range(vec!["2021-12-01", "2024-01-31"]))
        .title(Title::new("PRs created per month"));
    plot.set_layout(layout);

    let html = plot.to_html();
    std::fs::write("plot.html", html).unwrap();
}

trait MonthExt {
    fn as_double_digit_str(&self) -> &'static str;
}

impl MonthExt for Month {
    fn as_double_digit_str(&self) -> &'static str {
        match self {
            Month::January => "01",
            Month::February => "02",
            Month::March => "03",
            Month::April => "04",
            Month::May => "05",
            Month::June => "06",
            Month::July => "07",
            Month::August => "08",
            Month::September => "09",
            Month::October => "10",
            Month::November => "11",
            Month::December => "12",
        }
    }
}

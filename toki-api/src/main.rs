use std::env;

use az_devops::RepoClient;
use itertools::Itertools;
use plotly::{common::Title, layout::Axis, Layout, Plot, Scatter};

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

    for (i, pr) in pull_requests.iter().rev().enumerate() {
        println!(
            "{}: {} by {} ({})",
            i,
            pr.title,
            pr.created_by.display_name,
            pr.created_at.date(),
        );
    }

    // group by year and month created
    let pull_requests_by_month = pull_requests
        .iter()
        .sorted_by(|a, b| b.created_at.cmp(&a.created_at))
        .rev()
        .group_by(|pr| format!("{}-{}", pr.created_at.year(), pr.created_at.month()))
        .into_iter()
        .map(|(key, group)| (key, group.count()))
        .collect::<Vec<(String, usize)>>();

    // use plotly to plot the data
    let date = pull_requests_by_month
        .clone()
        .into_iter()
        .map(|(month, _)| month)
        .collect::<Vec<_>>();
    let count = pull_requests_by_month
        .iter()
        .map(|(_, count)| count.to_string())
        .collect::<Vec<_>>();

    let trace = Scatter::new(date, count);

    let mut plot = Plot::new();
    plot.add_trace(trace);

    let layout = Layout::new()
        .x_axis(Axis::new().range(vec!["2021-12-01", "2024-01-31"]))
        .title(Title::new("PRs created per month"));
    plot.set_layout(layout);

    let html = plot.to_html();
    std::fs::write("plot.html", html).unwrap();
}

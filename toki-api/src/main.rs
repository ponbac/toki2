use std::env;

use az_devops::RepoClient;

#[tokio::main]
async fn main() {
    dotenvy::from_filename("./toki-api/.env.local").ok();

    println!("Hello, world!");

    let organization = env::var("ADO_ORGANIZATION").unwrap();
    let project = env::var("ADO_PROJECT").unwrap();
    let repo_name = env::var("ADO_REPO").unwrap();
    let token = env::var("ADO_TOKEN").unwrap();

    let repo_client = RepoClient::new(&repo_name, &organization, &project, &token)
        .await
        .unwrap();

    let pull_requests = repo_client.get_open_pull_requests().await.unwrap();

    for pr in &pull_requests {
        let pr = pr.clone();

        println!(
            "PR: {} by {} ({}, {})",
            pr.title,
            pr.created_by.display_name,
            pr.created_by.unique_name,
            pr.created_by.avatar_url.unwrap()
        );
    }
}

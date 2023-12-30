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

    let all_pull_requests = repo_client.get_all_pull_requests().await.unwrap();

    println!("\nAll pull requests:");
    for pr in &all_pull_requests {
        let pr = pr.clone();

        println!(
            "PR: {} by {} ({}, {})",
            pr.title,
            pr.created_by.display_name,
            pr.created_by.unique_name,
            pr.created_by.avatar_url.unwrap()
        );
    }

    println!("\nFound {} pull requests", all_pull_requests.len());
}

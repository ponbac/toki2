use std::env;

use az_devops::RepoClient;

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
}

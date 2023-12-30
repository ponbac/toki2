use std::env;

use azure_devops_rust_api::git::{self, models::GitPullRequest};

mod utils;

pub async fn git_test() -> Result<Vec<GitPullRequest>, Box<dyn std::error::Error>> {
    // Get authentication credential
    let credential = utils::get_credential();

    // Get ADO server configuration via environment variables
    let organization = env::var("ADO_ORGANIZATION")?;
    let project = env::var("ADO_PROJECT")?;
    let repo_name = env::var("ADO_REPO")?;

    // Create a git client
    let git_client = git::ClientBuilder::new(credential).build();

    // Get all repositories in the specified organization/project
    let Some(repo) = git_client
        .repositories_client()
        .list(&organization, &project)
        .await?
        .value
        .iter()
        .find(|repo| repo.name == repo_name)
        .cloned()
    else {
        println!("Repo not found");
        return Err("Repo not found".into());
    };

    let pull_requests = git_client
        .pull_requests_client()
        .get_pull_requests(&organization, &repo.id, &project)
        .await?
        .value;

    for pr in &pull_requests {
        let pr = pr.clone();

        println!(
            "PR: {:?} by {:?}",
            pr.title.unwrap(),
            pr.created_by.graph_subject_base.display_name.unwrap()
        );
    }

    Ok(pull_requests)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_git_test() {
        dotenvy::from_filename(".env.local").ok();

        git_test().await.unwrap();
    }
}

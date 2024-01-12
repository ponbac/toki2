use std::collections::HashMap;

use az_devops::RepoClient;
use serde::Deserialize;
use sqlx::PgPool;

#[derive(Debug, Deserialize)]
pub struct RepositoryConfig {
    pub id: i32,
    pub organization: String,
    pub project: String,
    pub repo_name: String,
    pub token: String,
}

pub async fn query_repositories(
    pool: PgPool,
) -> Result<Vec<RepositoryConfig>, Box<dyn std::error::Error>> {
    let repos = sqlx::query_as!(
        RepositoryConfig,
        r#"
        SELECT id, organization, project, repo_name, token
        FROM repositories
        "#
    )
    .fetch_all(&pool)
    .await?;

    Ok(repos)
}

pub async fn repo_configs_to_clients(
    repo_configs: Vec<RepositoryConfig>,
) -> Result<HashMap<String, RepoClient>, Box<dyn std::error::Error>> {
    let mut repos = HashMap::new();
    for repo in repo_configs {
        let repo_client = RepoClient::new(
            &repo.repo_name,
            &repo.organization,
            &repo.project,
            &repo.token,
        )
        .await?;

        repos.insert(repo.repo_name.to_lowercase(), repo_client);
    }

    Ok(repos)
}

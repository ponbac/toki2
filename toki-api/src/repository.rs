use std::{collections::HashMap, fmt::Display};

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

impl RepositoryConfig {
    pub async fn to_client(&self) -> Result<RepoClient, Box<dyn std::error::Error>> {
        let repo_client = RepoClient::new(
            &self.repo_name,
            &self.organization,
            &self.project,
            &self.token,
        )
        .await?;

        Ok(repo_client)
    }

    pub fn key(&self) -> RepoKey {
        RepoKey {
            organization: self.organization.clone(),
            project: self.project.clone(),
            repo_name: self.repo_name.clone(),
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct RepoKey {
    pub organization: String,
    pub project: String,
    pub repo_name: String,
}

impl Display for RepoKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}/{}/{}",
            self.organization, self.project, self.repo_name
        )
    }
}

impl RepoKey {
    pub fn new(organization: &str, project: &str, repo_name: &str) -> Self {
        Self {
            organization: organization.to_owned(),
            project: project.to_owned(),
            repo_name: repo_name.to_owned(),
        }
    }
}

pub async fn query_repositories(
    pool: &PgPool,
) -> Result<Vec<RepositoryConfig>, Box<dyn std::error::Error>> {
    let repos = sqlx::query_as!(
        RepositoryConfig,
        r#"
        SELECT id, organization, project, repo_name, token
        FROM repositories
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(repos)
}

pub async fn insert_repository(
    pool: &PgPool,
    organization: &str,
    project: &str,
    repo_name: &str,
    token: &str,
) -> Result<i32, Box<dyn std::error::Error>> {
    let repo_id = sqlx::query!(
        r#"
        INSERT INTO repositories (organization, project, repo_name, token)
        VALUES ($1, $2, $3, $4)
        RETURNING id
        "#,
        organization,
        project,
        repo_name,
        token
    )
    .fetch_one(pool)
    .await?
    .id;
    tracing::info!(
        "Added repository to DB: {}/{}/{}",
        organization,
        project,
        repo_name
    );

    Ok(repo_id)
}

pub async fn repo_configs_to_clients(
    repo_configs: Vec<RepositoryConfig>,
) -> Result<HashMap<RepoKey, RepoClient>, Box<dyn std::error::Error>> {
    let mut repos = HashMap::new();
    for repo in repo_configs {
        repos.insert(repo.key(), repo.to_client().await?);
    }

    Ok(repos)
}

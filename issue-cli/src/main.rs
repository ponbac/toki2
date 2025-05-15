use az_devops::RepoClient;
use clap::Parser;

#[derive(Parser)]
#[command(name = "issue-cli", about = "Automate DevOps issues")]
struct Opts {
    /// The work-item (issue) ID to complete
    issue_id: u64,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::from_filename(".env.local").ok();

    let opts = Opts::parse();
    println!("Issue ID: {}", opts.issue_id);

    let repo = std::env::var("ADO_REPO").unwrap();
    let organization = std::env::var("ADO_ORGANIZATION").unwrap();
    let project = std::env::var("ADO_PROJECT").unwrap();
    let token = std::env::var("ADO_TOKEN").unwrap();

    let repo_client = RepoClient::new(&repo, &organization, &project, &token)
        .await
        .map_err(|e| anyhow::anyhow!("Error creating repo client: {}", e))?;

    let work_items = repo_client
        .get_work_items(vec![opts.issue_id as i32])
        .await
        .map_err(|e| anyhow::anyhow!("Error getting work items: {}", e))?;

    println!(
        "Work items: {:?}",
        work_items
            .iter()
            .map(|w| (w.title.clone(), w.description.clone()))
            .collect::<Vec<(String, String)>>()
    );

    Ok(())
}

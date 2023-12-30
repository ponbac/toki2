use az_devops::git_test;

#[tokio::main]
async fn main() {
    dotenvy::from_filename(".env.local").ok();

    println!("Hello, world!");

    let prs = git_test().await.unwrap();
    println!("prs: {:?}", prs);
}

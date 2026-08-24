use async_trait::async_trait;
use futures_util::{stream, StreamExt, TryStreamExt};

use crate::domain::{
    ports::outbound::{PullRequestProvider, PullRequestProviderError},
    PullRequest, PullRequestComment, PullRequestCommentType, PullRequestCommit,
    PullRequestCommitAuthor, PullRequestIdentity, PullRequestMergeStatus, PullRequestReviewer,
    PullRequestThread, PullRequestThreadStatus, PullRequestVote, PullRequestWorkItemRef, RepoKey,
};

use super::AzureDevOpsUrl;

/// Azure DevOps adapter for provider-neutral pull request snapshots.
pub struct AzureDevOpsPullRequestAdapter {
    client: az_devops::RepoClient,
    key: RepoKey,
}

impl AzureDevOpsPullRequestAdapter {
    const FETCH_CONCURRENCY: usize = 10;

    pub fn new(client: az_devops::RepoClient) -> Self {
        let key = RepoKey::new(client.organization(), client.project(), client.repo_name());
        Self { client, key }
    }
}

#[async_trait]
impl PullRequestProvider for AzureDevOpsPullRequestAdapter {
    fn repository(&self) -> &RepoKey {
        &self.key
    }

    async fn get_open_pull_requests(&self) -> Result<Vec<PullRequest>, PullRequestProviderError> {
        let base_pull_requests = self
            .client
            .get_open_pull_requests()
            .await
            .map_err(|_| PullRequestProviderError::PullRequests)?;

        let pull_request_ids = base_pull_requests
            .iter()
            .map(|pull_request| pull_request.id)
            .collect::<Vec<_>>();
        let mut work_items_by_pull_request = self
            .client
            .get_work_items_for_pull_requests(&pull_request_ids)
            .await
            .map_err(|_| PullRequestProviderError::WorkItems)?;

        let requests = base_pull_requests
            .into_iter()
            .enumerate()
            .map(|(index, pull_request)| {
                let work_items = work_items_by_pull_request
                    .remove(&pull_request.id)
                    .unwrap_or_default();
                (index, pull_request, work_items)
            });

        let mut hydrated = stream::iter(requests)
            .map(|(index, pull_request, work_items)| {
                let client = self.client.clone();
                let key = self.key.clone();
                async move {
                    let (commits, threads) = tokio::try_join!(
                        async {
                            pull_request
                                .commits(&client)
                                .await
                                .map_err(|_| PullRequestProviderError::Commits)
                        },
                        async {
                            pull_request
                                .threads(&client)
                                .await
                                .map_err(|_| PullRequestProviderError::Threads)
                        },
                    )?;

                    Ok::<_, PullRequestProviderError>((
                        index,
                        to_domain_pull_request(pull_request, threads, commits, work_items, &key),
                    ))
                }
            })
            .buffer_unordered(Self::FETCH_CONCURRENCY)
            .try_collect::<Vec<_>>()
            .await?;
        hydrated.sort_by_key(|(index, _)| *index);

        Ok(hydrated
            .into_iter()
            .map(|(_, pull_request)| pull_request)
            .collect())
    }

    async fn get_identities(&self) -> Result<Vec<PullRequestIdentity>, PullRequestProviderError> {
        self.client
            .get_git_identities()
            .await
            .map(|identities| identities.into_iter().map(to_domain_identity).collect())
            .map_err(|_| PullRequestProviderError::Identities)
    }
}

fn to_domain_pull_request(
    pull_request: az_devops::PullRequest,
    threads: Vec<az_devops::Thread>,
    commits: Vec<az_devops::GitCommitRef>,
    work_items: Vec<az_devops::WorkItem>,
    key: &RepoKey,
) -> PullRequest {
    let pull_request_id = pull_request.id.to_string();
    let canonical_url = AzureDevOpsUrl::PullRequest {
        org: &key.organization,
        project: &key.project,
        repo: &key.repo_name,
        id: &pull_request_id,
    };

    PullRequest {
        url: canonical_url.to_string(),
        id: pull_request.id,
        title: pull_request.title,
        description: pull_request.description,
        created_by: to_domain_identity(pull_request.created_by),
        created_at: pull_request.created_at,
        source_branch: pull_request.source_branch,
        target_branch: pull_request.target_branch,
        is_draft: pull_request.is_draft,
        merge_status: pull_request.merge_status.map(to_domain_merge_status),
        reviewers: pull_request
            .reviewers
            .into_iter()
            .map(to_domain_reviewer)
            .collect(),
        threads: threads
            .into_iter()
            .map(|thread| to_domain_thread(thread, &canonical_url))
            .collect(),
        commits: commits.into_iter().map(to_domain_commit).collect(),
        work_items: work_items
            .into_iter()
            .map(|work_item| to_domain_work_item_ref(work_item, key))
            .collect(),
    }
}

fn to_domain_identity(identity: az_devops::Identity) -> PullRequestIdentity {
    PullRequestIdentity {
        id: identity.id,
        display_name: identity.display_name,
        unique_name: identity.unique_name,
        avatar_url: identity.avatar_url,
    }
}

fn to_domain_reviewer(reviewer: az_devops::IdentityWithVote) -> PullRequestReviewer {
    PullRequestReviewer {
        identity: to_domain_identity(reviewer.identity),
        vote: reviewer.vote.map(to_domain_vote),
        has_declined: reviewer.has_declined,
        is_required: reviewer.is_required,
        is_flagged: reviewer.is_flagged,
    }
}

fn to_domain_vote(vote: az_devops::Vote) -> PullRequestVote {
    match vote {
        az_devops::Vote::NoResponse => PullRequestVote::NoResponse,
        az_devops::Vote::Approved => PullRequestVote::Approved,
        az_devops::Vote::ApprovedWithSuggestions => PullRequestVote::ApprovedWithSuggestions,
        az_devops::Vote::WaitingForAuthor => PullRequestVote::WaitingForAuthor,
        az_devops::Vote::Rejected => PullRequestVote::Rejected,
    }
}

fn to_domain_merge_status(status: az_devops::MergeStatus) -> PullRequestMergeStatus {
    match status {
        az_devops::MergeStatus::NotSet => PullRequestMergeStatus::NotSet,
        az_devops::MergeStatus::Queued => PullRequestMergeStatus::Queued,
        az_devops::MergeStatus::Conflicts => PullRequestMergeStatus::Conflicts,
        az_devops::MergeStatus::Succeeded => PullRequestMergeStatus::Succeeded,
        az_devops::MergeStatus::RejectedByPolicy => PullRequestMergeStatus::RejectedByPolicy,
        az_devops::MergeStatus::Failure => PullRequestMergeStatus::Failure,
    }
}

fn to_domain_thread(
    thread: az_devops::Thread,
    pull_request_url: &AzureDevOpsUrl<'_>,
) -> PullRequestThread {
    let thread_id = thread.id;
    PullRequestThread {
        id: thread_id,
        comments: thread
            .comments
            .into_iter()
            .map(|comment| to_domain_comment(comment, thread_id, pull_request_url))
            .collect(),
        status: thread.status.map(to_domain_thread_status),
        is_deleted: thread.is_deleted,
        last_updated_at: thread.last_updated_at,
        published_at: thread.published_at,
    }
}

fn to_domain_comment(
    comment: az_devops::Comment,
    thread_id: i32,
    pull_request_url: &AzureDevOpsUrl<'_>,
) -> PullRequestComment {
    let comment_type = comment.comment_type.map(to_domain_comment_type);
    let is_system = comment_type == Some(PullRequestCommentType::System)
        || comment.author.display_name == "Azure Pipelines Test Service";

    PullRequestComment {
        id: comment.id,
        author: to_domain_identity(comment.author),
        content: comment.content,
        comment_type,
        is_system,
        is_deleted: comment.is_deleted,
        published_at: comment.published_at,
        liked_by: comment
            .liked_by
            .into_iter()
            .map(to_domain_identity)
            .collect(),
        url: pull_request_url.pull_request_comment_url(thread_id, comment.id),
    }
}

fn to_domain_comment_type(comment_type: az_devops::CommentType) -> PullRequestCommentType {
    match comment_type {
        az_devops::CommentType::Unknown => PullRequestCommentType::Unknown,
        az_devops::CommentType::Text => PullRequestCommentType::Text,
        az_devops::CommentType::CodeChange => PullRequestCommentType::CodeChange,
        az_devops::CommentType::System => PullRequestCommentType::System,
    }
}

fn to_domain_thread_status(status: az_devops::ThreadStatus) -> PullRequestThreadStatus {
    match status {
        az_devops::ThreadStatus::Unknown => PullRequestThreadStatus::Unknown,
        az_devops::ThreadStatus::Active => PullRequestThreadStatus::Active,
        az_devops::ThreadStatus::Fixed => PullRequestThreadStatus::Fixed,
        az_devops::ThreadStatus::WontFix => PullRequestThreadStatus::WontFix,
        az_devops::ThreadStatus::Closed => PullRequestThreadStatus::Closed,
        az_devops::ThreadStatus::ByDesign => PullRequestThreadStatus::ByDesign,
        az_devops::ThreadStatus::Pending => PullRequestThreadStatus::Pending,
    }
}

fn to_domain_work_item_ref(
    work_item: az_devops::WorkItem,
    key: &RepoKey,
) -> PullRequestWorkItemRef {
    let id = work_item.id.to_string();
    PullRequestWorkItemRef {
        url: AzureDevOpsUrl::WorkItem {
            org: &key.organization,
            project: &key.project,
            id: &id,
        }
        .to_string(),
        id,
        title: work_item.title,
        parent_id: work_item.parent_id.map(|parent_id| parent_id.to_string()),
        priority: work_item.priority,
    }
}

fn to_domain_commit(commit: az_devops::GitCommitRef) -> PullRequestCommit {
    PullRequestCommit {
        author: commit.author.map(|author| PullRequestCommitAuthor {
            date: author.date,
            email: author.email,
            name: author.name,
        }),
        comment: commit.comment,
        commit_id: commit.commit_id,
        committer: commit.committer.map(|committer| PullRequestCommitAuthor {
            date: committer.date,
            email: committer.email,
            name: committer.name,
        }),
        url: commit.remote_url.or(commit.url),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linked_work_item_url_is_built_by_the_provider_adapter() {
        let work_item = az_devops::WorkItem {
            id: 42,
            parent_id: Some(7),
            title: "Fix login".to_string(),
            state: "Active".to_string(),
            board_column: None,
            item_type: "Bug".to_string(),
            priority: Some(1),
            created_at: time::OffsetDateTime::now_utc(),
            changed_at: time::OffsetDateTime::now_utc(),
            assigned_to: None,
            created_by: None,
            relations: vec![],
            description: None,
            repro_steps: None,
            acceptance_criteria: None,
            iteration_path: None,
            area_path: None,
            tags: None,
        };
        let key = RepoKey::new("org", "proj", "repo");

        let mapped = to_domain_work_item_ref(work_item, &key);

        assert_eq!(mapped.id, "42");
        assert_eq!(mapped.parent_id.as_deref(), Some("7"));
        assert_eq!(
            mapped.url,
            "https://dev.azure.com/org/proj/_workitems/edit/42"
        );
    }

    #[test]
    fn incomplete_commits_are_preserved_without_inventing_values() {
        let commits = vec![az_devops::GitCommitRef::new()];

        let mapped: Vec<_> = commits.into_iter().map(to_domain_commit).collect();

        assert_eq!(mapped.len(), 1);
        assert_eq!(
            mapped[0],
            PullRequestCommit {
                author: None,
                comment: None,
                commit_id: None,
                committer: None,
                url: None,
            }
        );
    }
}

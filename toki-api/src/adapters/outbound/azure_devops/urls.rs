use std::fmt;
use url::Url;

/// Centralized Azure DevOps web URL construction.
///
/// All provider-specific URL knowledge lives here so that conversion
/// and adapter code can build URLs without scattering `format!` macros.
pub(crate) enum AzureDevOpsUrl<'a> {
    WorkItem {
        org: &'a str,
        project: &'a str,
        id: &'a str,
    },
    PullRequest {
        org: &'a str,
        project: &'a str,
        repo: &'a str,
        id: &'a str,
    },
}

impl fmt::Display for AzureDevOpsUrl<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AzureDevOpsUrl::WorkItem { org, project, id } => {
                write!(
                    f,
                    "https://dev.azure.com/{org}/{project}/_workitems/edit/{id}"
                )
            }
            AzureDevOpsUrl::PullRequest {
                org,
                project,
                repo,
                id,
            } => {
                write!(
                    f,
                    "https://dev.azure.com/{org}/{project}/_git/{repo}/pullrequest/{id}"
                )
            }
        }
    }
}

impl AzureDevOpsUrl<'_> {
    pub(crate) fn pull_request_comment_url(&self, discussion_id: i32, comment_id: i64) -> String {
        match self {
            AzureDevOpsUrl::PullRequest { .. } => {
                append_pull_request_comment_params(&self.to_string(), discussion_id, comment_id)
            }
            AzureDevOpsUrl::WorkItem { .. } => {
                panic!("pull_request_comment_url called on non-pull-request URL variant")
            }
        }
    }
}

fn append_pull_request_comment_params(pr_url: &str, discussion_id: i32, comment_id: i64) -> String {
    let discussion_id = discussion_id.to_string();
    let comment_id = comment_id.to_string();

    let mut parsed_url = Url::parse(pr_url)
        .expect("AzureDevOpsUrl::PullRequest should always render a valid absolute URL");
    parsed_url
        .query_pairs_mut()
        .append_pair("discussionId", &discussion_id);
    parsed_url.set_fragment(Some(&comment_id));
    parsed_url.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_work_item_url() {
        let url = AzureDevOpsUrl::WorkItem {
            org: "myorg",
            project: "myproject",
            id: "12345",
        };
        assert_eq!(
            url.to_string(),
            "https://dev.azure.com/myorg/myproject/_workitems/edit/12345"
        );
    }

    #[test]
    fn test_pull_request_url() {
        let url = AzureDevOpsUrl::PullRequest {
            org: "myorg",
            project: "myproject",
            repo: "myrepo",
            id: "42",
        };
        assert_eq!(
            url.to_string(),
            "https://dev.azure.com/myorg/myproject/_git/myrepo/pullrequest/42"
        );
    }

    #[test]
    fn test_pull_request_comment_url_without_existing_query() {
        let url = AzureDevOpsUrl::PullRequest {
            org: "org",
            project: "project",
            repo: "repo",
            id: "2310",
        }
        .pull_request_comment_url(38706, 1770865798);
        assert_eq!(
            url,
            "https://dev.azure.com/org/project/_git/repo/pullrequest/2310?discussionId=38706#1770865798"
        );
    }

    #[test]
    fn test_pull_request_comment_url_with_existing_query() {
        let url = append_pull_request_comment_params(
            "https://dev.azure.com/org/project/_git/repo/pullrequest/2310?view=files",
            38706,
            1770865798,
        );
        assert_eq!(
            url,
            "https://dev.azure.com/org/project/_git/repo/pullrequest/2310?view=files&discussionId=38706#1770865798"
        );
    }

    #[test]
    fn test_pull_request_comment_url_preserves_existing_fragment() {
        let url = append_pull_request_comment_params(
            "https://dev.azure.com/org/project/_git/repo/pullrequest/2310#old",
            38706,
            1770865798,
        );
        assert_eq!(
            url,
            "https://dev.azure.com/org/project/_git/repo/pullrequest/2310?discussionId=38706#1770865798"
        );
    }

    #[test]
    #[should_panic(expected = "pull_request_comment_url called on non-pull-request URL variant")]
    fn test_pull_request_comment_url_panics_for_work_item_variant() {
        let _ = AzureDevOpsUrl::WorkItem {
            org: "org",
            project: "project",
            id: "123",
        }
        .pull_request_comment_url(38706, 1770865798);
    }
}

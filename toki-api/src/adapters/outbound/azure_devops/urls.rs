use std::fmt;

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
}

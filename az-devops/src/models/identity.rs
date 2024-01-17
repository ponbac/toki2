use azure_devops_rust_api::git::models::{IdentityRef, IdentityRefWithVote};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub id: String,
    pub display_name: String,
    pub unique_name: String,
    pub avatar_url: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Vote {
    Approved,
    ApprovedWithSuggestions,
    NoResponse,
    WaitingForAuthor,
    Rejected,
}

impl From<i64> for Vote {
    fn from(vote: i64) -> Self {
        match vote {
            10 => Self::Approved,
            5 => Self::ApprovedWithSuggestions,
            0 => Self::NoResponse,
            -5 => Self::WaitingForAuthor,
            -10 => Self::Rejected,
            _ => panic!("Invalid vote value"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub struct IdentityWithVote {
    pub identity: Identity,
    pub vote: Option<Vote>,
    pub has_declined: Option<bool>,
    pub is_required: Option<bool>,
    pub is_flagged: Option<bool>,
}

impl From<IdentityRef> for Identity {
    fn from(identity: IdentityRef) -> Self {
        Self {
            id: identity.id,
            display_name: identity.graph_subject_base.display_name.unwrap(),
            unique_name: identity.unique_name.unwrap(),
            avatar_url: identity
                .graph_subject_base
                .links
                .unwrap()
                .get("avatar")
                .map(|obj| {
                    Value::to_string(obj.get("href").unwrap())
                        .trim_matches('"')
                        .to_string()
                }),
        }
    }
}

impl From<IdentityRefWithVote> for IdentityWithVote {
    fn from(identity: IdentityRefWithVote) -> Self {
        Self {
            identity: identity.identity_ref.into(),
            vote: identity.vote.map(|vote| vote.into()),
            has_declined: identity.has_declined,
            is_required: identity.is_required,
            is_flagged: identity.is_flagged,
        }
    }
}

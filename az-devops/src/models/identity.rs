use azure_devops_rust_api::git::models::IdentityRef;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Identity {
    pub id: String,
    pub display_name: String,
    pub unique_name: String,
    pub avatar_url: Option<String>,
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
                .map(Value::to_string),
        }
    }
}

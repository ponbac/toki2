use serde::Serialize;
use time::OffsetDateTime;

use super::UserId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvatarImage {
    pub bytes: Vec<u8>,
    pub mime_type: String,
}

impl AvatarImage {
    pub fn new(bytes: Vec<u8>, mime_type: impl Into<String>) -> Self {
        Self {
            bytes,
            mime_type: mime_type.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AvatarIdentityRecord {
    pub user_id: UserId,
    pub email: String,
    pub updated_at: OffsetDateTime,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AvatarOverride {
    pub email: String,
    pub avatar_url: String,
}

impl AvatarOverride {
    pub fn new(email: impl Into<String>, avatar_url: impl Into<String>) -> Self {
        Self {
            email: email.into(),
            avatar_url: avatar_url.into(),
        }
    }
}

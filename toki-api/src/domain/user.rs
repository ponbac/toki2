use std::fmt;

use crate::domain::models::UserId;
use axum_login::AuthUser;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum Role {
    Admin,
    User,
}

impl From<String> for Role {
    fn from(role: String) -> Self {
        match role.as_str() {
            "Admin" => Role::Admin,
            "User" => Role::User,
            _ => Role::User,
        }
    }
}

impl fmt::Display for Role {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let role_str = match self {
            Role::Admin => "Admin",
            Role::User => "User",
        };
        write!(f, "{role_str}")
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct User {
    pub id: UserId,
    pub email: String,
    pub full_name: String,
    pub picture: String,
    pub roles: Vec<Role>,
    #[serde(skip)]
    pub session_auth_hash: String,
}

/// The identity data that request authorization may expose to application code.
///
/// Provider credentials and session internals deliberately do not belong to the
/// request principal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserPrincipal {
    pub id: UserId,
    pub email: String,
    pub roles: Vec<Role>,
}

impl From<&User> for UserPrincipal {
    fn from(user: &User) -> Self {
        Self {
            id: user.id,
            email: user.email.clone(),
            roles: user.roles.clone(),
        }
    }
}

impl fmt::Debug for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("email", &self.email)
            .field("full_name", &self.full_name)
            .field("picture", &self.picture)
            .field("roles", &self.roles)
            .finish()
    }
}

impl AuthUser for User {
    type Id = i64;

    fn id(&self) -> Self::Id {
        self.id.as_i32().into()
    }

    fn session_auth_hash(&self) -> &[u8] {
        self.session_auth_hash.as_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serialized_user_contains_only_public_profile_fields() {
        let user = User {
            id: UserId::new(7),
            email: "ada@example.com".to_string(),
            full_name: "Ada Lovelace".to_string(),
            picture: "https://example.com/ada.png".to_string(),
            roles: vec![Role::User],
            session_auth_hash: "session-secret".to_string(),
        };

        let serialized = serde_json::to_value(user).expect("user should serialize");

        assert_eq!(serialized["email"], "ada@example.com");
        assert!(serialized.get("accessToken").is_none());
        assert!(serialized.get("sessionAuthHash").is_none());
    }
}

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
    pub access_token: String,
    pub roles: Vec<Role>,
    #[serde(skip)]
    pub session_auth_hash: String,
}

impl fmt::Debug for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("User")
            .field("id", &self.id)
            .field("email", &self.email)
            .field("full_name", &self.full_name)
            .field("picture", &self.picture)
            .field("roles", &self.roles)
            .field("access_token", &"[redacted]")
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

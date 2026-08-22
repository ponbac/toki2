use std::fmt;

use rand::RngCore;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;

use super::{ApiTokenId, UserId};

const TOKEN_PREFIX: &str = "toki_";
const TOKEN_SECRET_BYTES: usize = 32;
const TOKEN_DISPLAY_PREFIX_LEN: usize = 12;
pub const MAX_TOKENS_PER_USER: usize = 20;
pub const MAX_TOKEN_NAME_LEN: usize = 64;

/// A trimmed, non-empty label for a personal API token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiTokenName(String);

impl ApiTokenName {
    pub fn parse(raw: &str) -> Option<Self> {
        let name = raw.trim();
        if name.is_empty()
            || name.chars().count() > MAX_TOKEN_NAME_LEN
            || name.chars().any(char::is_control)
        {
            return None;
        }

        Some(Self(name.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A stored personal access token. The secret is never retained after creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiToken {
    pub id: ApiTokenId,
    pub user_id: UserId,
    pub name: ApiTokenName,
    pub prefix: String,
    pub created_at: OffsetDateTime,
}

/// A fixed-width digest used as the persisted token lookup key.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ApiTokenHash([u8; 32]);

impl ApiTokenHash {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

/// Validated token material ready to cross the persistence boundary.
pub struct NewApiToken {
    name: ApiTokenName,
    prefix: String,
    hash: ApiTokenHash,
}

impl NewApiToken {
    pub fn new(name: ApiTokenName, secret: &ApiTokenSecret) -> Self {
        Self {
            name,
            prefix: secret.prefix().to_string(),
            hash: secret.hash(),
        }
    }

    pub fn name(&self) -> &ApiTokenName {
        &self.name
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn hash(&self) -> &ApiTokenHash {
        &self.hash
    }
}

/// Newly issued token. `secret` is shown once to the caller and then discarded.
pub struct IssuedApiToken {
    pub token: ApiToken,
    pub secret: ApiTokenSecret,
}

/// Opaque `toki_<hex>` credential. Debug redacts the value.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiTokenSecret(String);

impl ApiTokenSecret {
    pub fn generate() -> Self {
        let mut bytes = [0u8; TOKEN_SECRET_BYTES];
        rand::thread_rng().fill_bytes(&mut bytes);
        Self(format!("{TOKEN_PREFIX}{}", encode_hex(&bytes)))
    }

    pub fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        let secret = trimmed.strip_prefix(TOKEN_PREFIX)?;
        if secret.len() != TOKEN_SECRET_BYTES * 2 {
            return None;
        }
        if !secret.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }

        Some(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn prefix(&self) -> &str {
        &self.0[..TOKEN_DISPLAY_PREFIX_LEN]
    }

    pub fn hash(&self) -> ApiTokenHash {
        ApiTokenHash(Sha256::digest(self.0.as_bytes()).into())
    }
}

impl fmt::Debug for ApiTokenSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ApiTokenSecret([redacted])")
    }
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_secret_round_trips_through_parse_and_hash() {
        let secret = ApiTokenSecret::generate();
        let parsed = ApiTokenSecret::parse(secret.as_str()).expect("generated secret should parse");
        assert_eq!(secret.as_str(), parsed.as_str());
        assert!(secret.hash() == parsed.hash());
        assert!(secret.as_str().starts_with("toki_"));
        assert_eq!(secret.prefix().len(), TOKEN_DISPLAY_PREFIX_LEN);
        assert_eq!(
            secret.prefix(),
            &secret.as_str()[..TOKEN_DISPLAY_PREFIX_LEN]
        );
    }

    #[test]
    fn parse_rejects_malformed_secrets() {
        assert!(ApiTokenSecret::parse("").is_none());
        assert!(ApiTokenSecret::parse("not-a-token").is_none());
        assert!(ApiTokenSecret::parse("toki_short").is_none());
        assert!(
            ApiTokenSecret::parse(&format!("toki_{}", "z".repeat(TOKEN_SECRET_BYTES * 2)))
                .is_none()
        );
    }

    #[test]
    fn parse_trims_whitespace() {
        let secret = ApiTokenSecret::generate();
        let padded = format!("  {}\n", secret.as_str());
        let parsed = ApiTokenSecret::parse(&padded).expect("whitespace should be trimmed");
        assert_eq!(parsed.as_str(), secret.as_str());
    }

    #[test]
    fn debug_does_not_contain_secret() {
        let secret = ApiTokenSecret::generate();
        let debug = format!("{secret:?}");
        assert!(!debug.contains(secret.as_str()));
        assert!(debug.contains("redacted"));
    }

    #[test]
    fn token_name_must_be_present_and_short() {
        assert_eq!(
            ApiTokenName::parse("  Omarchy bar  ")
                .as_ref()
                .map(ApiTokenName::as_str),
            Some("Omarchy bar")
        );
        assert!(ApiTokenName::parse("").is_none());
        assert!(ApiTokenName::parse("   ").is_none());
        assert!(ApiTokenName::parse(&"n".repeat(MAX_TOKEN_NAME_LEN + 1)).is_none());
        assert!(ApiTokenName::parse("line\nbreak").is_none());
        assert!(ApiTokenName::parse(&"🦀".repeat(MAX_TOKEN_NAME_LEN)).is_some());
        assert!(ApiTokenName::parse(&"🦀".repeat(MAX_TOKEN_NAME_LEN + 1)).is_none());
    }
}

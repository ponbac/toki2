use std::fmt;
use std::ops::Deref;
use thiserror::Error;

/// A validated email address.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Email(String);

#[derive(Error, Debug, PartialEq)]
pub enum EmailError {
    #[error("'{0}' is not a valid email: must contain only one '@'")]
    InvalidFormat(String),
    #[error("'{0}' is not a valid email: missing local part")]
    MissingLocalPart(String),
    #[error("'{0}' is not a valid email: invalid domain part")]
    InvalidDomainPart(String),
}

impl Email {
    /// Normalizes an identity/email lookup key for avatar and cache lookups.
    ///
    /// If the input is a valid email, this returns the canonicalized email.
    /// Otherwise, it falls back to a trimmed/lowercased raw string.
    pub fn normalize_lookup_key(value: &str) -> Option<String> {
        let normalized = value.trim();
        if normalized.is_empty() {
            return None;
        }

        match Self::try_from(normalized) {
            Ok(email) => Some(email.0),
            Err(_) => Some(normalized.to_lowercase()),
        }
    }
}

impl TryFrom<&str> for Email {
    type Error = EmailError;

    /// Validates a string and converts it into an `Email`.
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let normalized = value.trim();
        let mut parts = normalized.split('@');
        let local_part = parts.next();
        let domain_part = parts.next();
        let extra_part = parts.next();

        if extra_part.is_some() {
            return Err(EmailError::InvalidFormat(value.to_string()));
        }

        if local_part.unwrap_or_default().trim().is_empty() {
            return Err(EmailError::MissingLocalPart(value.to_string()));
        }

        let domain = domain_part.unwrap_or_default();
        if domain.trim().is_empty()
            || !domain.contains('.')
            || domain.starts_with('.')
            || domain.ends_with('.')
        {
            return Err(EmailError::InvalidDomainPart(value.to_string()));
        }

        Ok(Self(normalized.to_lowercase()))
    }
}

impl Deref for Email {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AsRef<str> for Email {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_email_is_accepted() {
        assert!(Email::try_from("test@example.com").is_ok());
    }

    #[test]
    fn missing_at_symbol_is_rejected() {
        assert_eq!(
            Email::try_from("testexample.com").unwrap_err(),
            EmailError::InvalidDomainPart("testexample.com".to_string())
        );
    }

    #[test]
    fn multiple_at_symbols_are_rejected() {
        assert_eq!(
            Email::try_from("test@@example.com").unwrap_err(),
            EmailError::InvalidFormat("test@@example.com".to_string())
        );
    }

    #[test]
    fn missing_local_part_is_rejected() {
        assert_eq!(
            Email::try_from("@example.com").unwrap_err(),
            EmailError::MissingLocalPart("@example.com".to_string())
        );
    }

    #[test]
    fn missing_domain_part_is_rejected() {
        assert_eq!(
            Email::try_from("test@").unwrap_err(),
            EmailError::InvalidDomainPart("test@".to_string())
        );
    }

    #[test]
    fn domain_part_must_contain_dot() {
        assert_eq!(
            Email::try_from("test@example").unwrap_err(),
            EmailError::InvalidDomainPart("test@example".to_string())
        );
    }

    #[test]
    fn email_is_canonicalized_to_lowercase_and_trimmed() {
        let email = Email::try_from("  USER@Example.com  ").expect("email should be valid");
        assert_eq!(email.as_ref(), "user@example.com");
    }

    #[test]
    fn normalize_lookup_key_uses_email_canonicalization_when_possible() {
        assert_eq!(
            Email::normalize_lookup_key(" USER@Example.com "),
            Some("user@example.com".to_string())
        );
    }

    #[test]
    fn normalize_lookup_key_falls_back_to_raw_lowercased_value() {
        assert_eq!(
            Email::normalize_lookup_key("  Display Name  "),
            Some("display name".to_string())
        );
    }

    #[test]
    fn normalize_lookup_key_rejects_empty_values() {
        assert_eq!(Email::normalize_lookup_key("   "), None);
    }
}

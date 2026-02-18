use serde::{Deserialize, Serialize};
use std::fmt;

/// A validated user identifier.
///
/// Wraps i32 to match the database SERIAL type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UserId(i32);

impl UserId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }

    pub fn as_i32(&self) -> i32 {
        self.0
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<i32> for UserId {
    fn from(id: i32) -> Self {
        Self(id)
    }
}

impl From<UserId> for i32 {
    fn from(id: UserId) -> Self {
        id.0
    }
}

impl AsRef<i32> for UserId {
    fn as_ref(&self) -> &i32 {
        &self.0
    }
}

/// A project identifier from the time tracking system.
///
/// Wraps String as Milltime uses string IDs like "300000000000241970".
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectId(String);

impl ProjectId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProjectId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for ProjectId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for ProjectId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for ProjectId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

/// An activity identifier from the time tracking system.
///
/// Wraps String as Milltime uses string IDs like "201201111420550010".
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActivityId(String);

impl ActivityId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ActivityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for ActivityId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for ActivityId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for ActivityId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

/// A timer/time entry identifier from the time tracking system.
///
/// Wraps String as Milltime uses string IDs like "300000000000463334".
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TimerId(String);

impl TimerId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TimerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl AsRef<str> for TimerId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl From<String> for TimerId {
    fn from(id: String) -> Self {
        Self(id)
    }
}

impl From<&str> for TimerId {
    fn from(id: &str) -> Self {
        Self(id.to_string())
    }
}

/// A local timer history record identifier (database SERIAL).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimerHistoryId(i32);

impl TimerHistoryId {
    pub fn new(id: i32) -> Self {
        Self(id)
    }

    /// Extract the raw i32 value (consistent with `UserId::as_i32()`).
    pub fn as_i32(&self) -> i32 {
        self.0
    }
}

impl fmt::Display for TimerHistoryId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<i32> for TimerHistoryId {
    fn from(id: i32) -> Self {
        Self(id)
    }
}

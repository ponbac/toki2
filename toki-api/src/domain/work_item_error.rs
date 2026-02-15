use thiserror::Error;

/// Errors that can occur during work item operations.
#[derive(Debug, Error)]
pub enum WorkItemError {
    #[error("Provider error: {0}")]
    ProviderError(String),
}

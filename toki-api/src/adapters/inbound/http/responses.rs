//! HTTP response projections exposed to browser and agent clients.

mod pull_requests;
mod time_tracking;
mod work_items;

use serde::Serialize;
use utoipa::ToSchema;

pub use pull_requests::*;
pub use time_tracking::*;
pub use work_items::*;

/// JSON error body returned by HTTP handlers.
#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorResponse {
    /// Human-readable error message.
    pub error: String,
}

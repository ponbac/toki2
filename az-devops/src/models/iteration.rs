use serde::{Deserialize, Serialize};
use time::OffsetDateTime;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Iteration {
    pub id: i32,
    pub name: String,
    pub path: String,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub start_date: Option<OffsetDateTime>,
    #[serde(default, with = "time::serde::rfc3339::option")]
    pub finish_date: Option<OffsetDateTime>,
}

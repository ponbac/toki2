use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDto {
    pub project_id: String,
    pub project_name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityDto {
    pub activity: String,
    pub activity_name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StartTimerRequest {
    pub project_id: Option<String>,
    pub activity_id: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTimerRequest {
    pub note: Option<String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateActiveTimerRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub project_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub activity_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(
        skip_serializing_if = "Option::is_none",
        with = "time::serde::rfc3339::option"
    )]
    pub start_time: Option<time::OffsetDateTime>,
}

impl UpdateActiveTimerRequest {
    pub fn selection(project_id: Option<String>, activity_id: Option<String>) -> Self {
        Self {
            project_id: Some(project_id),
            activity_id: Some(activity_id),
            ..Self::default()
        }
    }

    pub fn note(note: String) -> Self {
        Self {
            note: Some(note),
            ..Self::default()
        }
    }

    pub fn replace(
        project_id: Option<String>,
        activity_id: Option<String>,
        note: String,
        start_time: Option<time::OffsetDateTime>,
    ) -> Self {
        Self {
            project_id: Some(project_id),
            activity_id: Some(activity_id),
            note: Some(note),
            start_time,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditEntryRequest<'a> {
    pub project_id: &'a str,
    pub activity_id: &'a str,
    pub start_time: String,
    pub end_time: String,
    pub note: &'a str,
}

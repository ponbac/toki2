use axum::{
    extract::{Query, State},
    http::StatusCode,
    Json,
};
use serde::Deserialize;

use crate::{
    adapters::inbound::http::{
        AbsenceDayDefaultResponse, AbsenceEntryResponse, AbsenceTypeResponse,
    },
    app_state::AppState,
    auth::AuthUser,
    domain::models::{AbsenceType, CreateAbsenceDay, CreateAbsencesRequest},
    observability::record_user_id,
    routes::ApiError,
};

#[derive(Debug, Deserialize)]
pub struct DateFilterQuery {
    from: String,
    to: String,
}

impl DateFilterQuery {
    fn date_range(&self) -> Result<(time::Date, time::Date), ApiError> {
        Ok((parse_date(&self.from)?, parse_date(&self.to)?))
    }
}

fn parse_date(s: &str) -> Result<time::Date, ApiError> {
    let format = time::format_description::parse("[year]-[month]-[day]").unwrap();
    time::Date::parse(s, &format)
        .map_err(|_| ApiError::bad_request(format!("could not parse date: {}", s)))
}

pub async fn get_absence_types(
    user: AuthUser,
    State(app_state): State<AppState>,
) -> Result<Json<Vec<AbsenceTypeResponse>>, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let types = service.get_absence_types().await?;

    Ok(Json(types.into_iter().map(Into::into).collect()))
}

pub async fn get_absences(
    user: AuthUser,
    State(app_state): State<AppState>,
    Query(query): Query<DateFilterQuery>,
) -> Result<Json<Vec<AbsenceEntryResponse>>, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let entries = service.get_absences(query.date_range()?).await?;

    Ok(Json(entries.into_iter().map(Into::into).collect()))
}

pub async fn get_absence_day_defaults(
    user: AuthUser,
    State(app_state): State<AppState>,
    Query(query): Query<DateFilterQuery>,
) -> Result<Json<Vec<AbsenceDayDefaultResponse>>, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let defaults = service
        .get_absence_day_defaults(query.date_range()?)
        .await?;

    Ok(Json(defaults.into_iter().map(Into::into).collect()))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAbsencesPayload {
    absence_type: AbsenceType,
    child: Option<String>,
    comment: String,
    days: Vec<CreateAbsenceDayPayload>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateAbsenceDayPayload {
    date: String,
    hours: f64,
}

impl CreateAbsencesPayload {
    fn into_request(self) -> Result<CreateAbsencesRequest, ApiError> {
        let days = self
            .days
            .into_iter()
            .map(CreateAbsenceDayPayload::into_day)
            .collect::<Result<Vec<_>, ApiError>>()?;

        Ok(CreateAbsencesRequest {
            absence_type: self.absence_type,
            child: self.child,
            comment: self.comment,
            days,
        })
    }
}

impl CreateAbsenceDayPayload {
    fn into_day(self) -> Result<CreateAbsenceDay, ApiError> {
        Ok(CreateAbsenceDay {
            date: parse_date(&self.date)?,
            hours: self.hours,
        })
    }
}

pub async fn create_absences(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(payload): Json<CreateAbsencesPayload>,
) -> Result<(StatusCode, Json<Vec<AbsenceEntryResponse>>), ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;

    let request = payload.into_request()?;
    let entries = service.create_absences(&request).await?;

    Ok((
        StatusCode::CREATED,
        Json(entries.into_iter().map(Into::into).collect()),
    ))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteAbsencePayload {
    absence_id: String,
    date: String,
}

pub async fn delete_absence(
    user: AuthUser,
    State(app_state): State<AppState>,
    Json(payload): Json<DeleteAbsencePayload>,
) -> Result<StatusCode, ApiError> {
    record_user_id(user.id);
    let service = app_state
        .time_tracking_factory
        .create_service(user.id)
        .await?;
    let date = parse_date(&payload.date)?;

    service.delete_absence(&payload.absence_id, date).await?;

    Ok(StatusCode::OK)
}

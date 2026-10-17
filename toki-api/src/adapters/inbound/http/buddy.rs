//! Timer buddies: the caller's buddy profile, with their buddies' levels and the XP
//! they earned today.
//!
//! Every malformed body is a `400` with the usual JSON error. Broken profile rules,
//! such as a partner before it unlocks, are a `409`.

use std::sync::Arc;

use axum::{
    extract::{FromRef, State},
    routing::get,
    Json, Router,
};
use axum_extra::extract::WithRejection;
use serde::{Deserialize, Serialize};
use time::{Date, OffsetDateTime};

use crate::{
    adapters::inbound::http::ai_subscriptions::iso_date,
    auth::AuthUser,
    domain::{
        models::{BuddyDay, BuddyOverview, BuddyProfileUpdate, SpeciesId, PARTNER_UNLOCK_LEVEL},
        ports::inbound::BuddyService,
        BuddyError,
    },
    routes::ApiError,
};

/// A JSON body whose rejection is a `400` JSON error.
type JsonBody<T> = WithRejection<Json<T>, ApiError>;

pub fn router<S>() -> Router<S>
where
    S: Clone + Send + Sync + 'static,
    Arc<dyn BuddyService>: FromRef<S>,
{
    Router::new().route("/", get(get_profile).put(update_profile))
}

/// The caller's profile, `null` before they enrol.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BuddyProfileEnvelope {
    profile: Option<BuddyProfileResponse>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BuddyProfileResponse {
    enabled: bool,
    battle_notifications: bool,
    /// In slot order, so the lead comes first.
    buddies: Vec<BuddyResponse>,
    partner_unlock_level: u8,
    today: BuddyTodayResponse,
    #[serde(with = "time::serde::rfc3339::option")]
    hours_synced_at: Option<OffsetDateTime>,
}

impl From<BuddyOverview> for BuddyProfileResponse {
    fn from(overview: BuddyOverview) -> Self {
        let buddies = overview
            .profile
            .buddies()
            .map(|buddy| {
                let progress = overview.progress(buddy);
                BuddyResponse {
                    slot: buddy.slot.number(),
                    species_id: buddy.species_id.as_i16(),
                    visible: buddy.visible,
                    level: progress.level,
                    xp: progress.xp,
                    level_xp: progress.level_xp,
                    next_level_xp: progress.next_level_xp,
                }
            })
            .collect();

        Self {
            enabled: overview.profile.enabled,
            battle_notifications: overview.profile.battle_notifications,
            buddies,
            partner_unlock_level: PARTNER_UNLOCK_LEVEL,
            today: overview.today.into(),
            hours_synced_at: overview.profile.hours_synced_at,
        }
    }
}

/// One buddy. `xp` is its own XP; `levelXp` and `nextLevelXp` bound its level, and
/// `nextLevelXp` is null at the highest level.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BuddyResponse {
    slot: i16,
    species_id: i16,
    visible: bool,
    level: u8,
    xp: i64,
    level_xp: i64,
    next_level_xp: Option<i64>,
}

/// The XP earned on today's local date.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BuddyTodayResponse {
    #[serde(with = "iso_date")]
    date: Date,
    logged_hours: f64,
    hours_xp: i64,
    battles: i64,
    battle_xp: i64,
}

impl From<BuddyDay> for BuddyTodayResponse {
    fn from(day: BuddyDay) -> Self {
        Self {
            date: day.date,
            logged_hours: day.logged_hours,
            hours_xp: day.hours_xp,
            battles: day.battles,
            battle_xp: day.battle_xp(),
        }
    }
}

/// Changes to the caller's profile; absent or null fields stay as they are. The first
/// update enrols the caller and needs `enabled` and `leadSpeciesId`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateBuddyProfileRequest {
    enabled: Option<bool>,
    battle_notifications: Option<bool>,
    lead_species_id: Option<i64>,
    partner_species_id: Option<i64>,
    partner_visible: Option<bool>,
}

impl UpdateBuddyProfileRequest {
    fn into_update(self) -> Result<BuddyProfileUpdate, BuddyError> {
        Ok(BuddyProfileUpdate {
            enabled: self.enabled,
            battle_notifications: self.battle_notifications,
            lead_species_id: self.lead_species_id.map(species_id).transpose()?,
            partner_species_id: self.partner_species_id.map(species_id).transpose()?,
            partner_visible: self.partner_visible,
        })
    }
}

/// A national dex number, which the service checks is a buddy species.
fn species_id(raw: i64) -> Result<SpeciesId, BuddyError> {
    i16::try_from(raw)
        .map(SpeciesId::new)
        .map_err(|_| BuddyError::Invalid(format!("there is no buddy species {raw}")))
}

/// The caller's buddy profile, after reading their recent logged hours.
async fn get_profile(
    user: AuthUser,
    State(service): State<Arc<dyn BuddyService>>,
) -> Result<Json<BuddyProfileEnvelope>, ApiError> {
    let overview = service.get_profile(&user.id).await?;
    Ok(Json(BuddyProfileEnvelope {
        profile: overview.map(Into::into),
    }))
}

/// Enrols the caller or changes their buddy settings.
async fn update_profile(
    user: AuthUser,
    State(service): State<Arc<dyn BuddyService>>,
    WithRejection(Json(request), _): JsonBody<UpdateBuddyProfileRequest>,
) -> Result<Json<BuddyProfileEnvelope>, ApiError> {
    let overview = service
        .update_profile(&user.id, request.into_update()?)
        .await?;
    Ok(Json(BuddyProfileEnvelope {
        profile: Some(overview.into()),
    }))
}

#[cfg(test)]
mod tests;

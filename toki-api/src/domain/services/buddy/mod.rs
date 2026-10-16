//! Timer buddies: enrolment, buddy settings and the XP earned from logged hours.
//!
//! Logged hours are read lazily: when a user opens their profile with their buddy
//! switched on, the hours of their recent days are read from time tracking, at most
//! once per `HOURS_SYNC_INTERVAL`, and stored as XP days. Other users' levels come
//! from what was stored when they last opened theirs, so no request ever waits for
//! someone else's time tracking.

use std::sync::Arc;

use async_trait::async_trait;
use time::{Date, OffsetDateTime};

use super::battle;
use crate::domain::{
    models::{
        hours_sync_due, hours_sync_window, xp_days, AiUsageTimeZone, BuddyClock, BuddyOverview,
        BuddyProfile, BuddyProfileUpdate, BuddyProgress, NewBuddyProfile, UserId,
        PARTNER_UNLOCK_LEVEL,
    },
    ports::{
        inbound::BuddyService,
        outbound::{BuddyRepository, LoggedHours},
    },
    BuddyError,
};

pub struct BuddyServiceImpl<R, H> {
    repository: Arc<R>,
    logged_hours: Arc<H>,
    time_zone: AiUsageTimeZone,
}

impl<R, H> BuddyServiceImpl<R, H> {
    /// `time_zone` defines the local days that hours count on.
    pub fn new(repository: Arc<R>, logged_hours: Arc<H>, time_zone: AiUsageTimeZone) -> Self {
        Self {
            repository,
            logged_hours,
            time_zone,
        }
    }
}

impl<R: BuddyRepository, H: LoggedHours> BuddyServiceImpl<R, H> {
    /// Stores the hours the user logged on each day of the sync window, days without
    /// hours included, unless a read started within `HOURS_SYNC_INTERVAL`. Returns when
    /// they were read, or `None` when they were not due, another request is reading
    /// them, or the user is not connected to time tracking.
    async fn sync_hours(
        &self,
        profile: &BuddyProfile,
        clock: BuddyClock,
    ) -> Result<Option<OffsetDateTime>, BuddyError> {
        // Claimed before the read, so a read that fails or hangs holds off the next
        // ones too, and concurrent requests read once.
        if !hours_sync_due(profile.hours_sync_attempted_at, clock.now)
            || !self
                .repository
                .claim_hours_sync(&profile.user_id, profile.hours_sync_attempted_at)
                .await?
        {
            return Ok(None);
        }
        let days = match hours_sync_window(profile.xp_since, clock.today, profile.hours_synced_at) {
            Some(window) => {
                // Without time tracking the stored hours stay as they are.
                let Some(hours) = self
                    .logged_hours
                    .daily_worked_hours(profile.user_id, window)
                    .await?
                else {
                    return Ok(None);
                };
                xp_days(window, &hours)
            }
            None => Vec::new(),
        };

        self.repository
            .save_logged_days(&profile.user_id, &days)
            .await
            .map(Some)
    }

    async fn overview(
        &self,
        profile: BuddyProfile,
        today: Date,
    ) -> Result<BuddyOverview, BuddyError> {
        let xp = self.repository.xp(&profile.user_id, today).await?;
        Ok(BuddyOverview {
            profile,
            total_xp: xp.tally.total_xp(),
            today: xp.today,
        })
    }
}

#[async_trait]
impl<R: BuddyRepository, H: LoggedHours> BuddyService for BuddyServiceImpl<R, H> {
    async fn get_profile(&self, user_id: &UserId) -> Result<Option<BuddyOverview>, BuddyError> {
        let Some(mut profile) = self.repository.profile(user_id).await? else {
            return Ok(None);
        };
        let clock = self.repository.clock(&self.time_zone).await?;
        // Hours are not read for a buddy that is switched off, so its levels stay put.
        let synced = if profile.enabled {
            self.sync_hours(&profile, clock).await
        } else {
            Ok(None)
        };
        // A profile never fails to load because time tracking is slow or down.
        match synced {
            Ok(Some(synced_at)) => profile.hours_synced_at = Some(synced_at),
            Ok(None) => {}
            Err(error) => tracing::warn!(
                %user_id,
                "Showing stored buddy XP, logged hours could not be synced: {error}"
            ),
        }

        self.overview(profile, clock.today).await.map(Some)
    }

    async fn update_profile(
        &self,
        user_id: &UserId,
        update: BuddyProfileUpdate,
    ) -> Result<BuddyOverview, BuddyError> {
        ensure_known_species(&update)?;
        let clock = self.repository.clock(&self.time_zone).await?;
        let profile = match self.repository.profile(user_id).await? {
            Some(profile) => profile,
            None => {
                let new_profile = enrolment(&update, clock.today)?;
                match self.repository.enrol(user_id, &new_profile).await? {
                    Some(profile) => return self.overview(profile, clock.today).await,
                    // A concurrent request enrolled the user: update that profile.
                    None => self
                        .repository
                        .profile(user_id)
                        .await?
                        .ok_or(BuddyError::NotEnrolled)?,
                }
            }
        };

        // Hours are not read while the buddy is switched off, so catch up before the
        // levels are shown or checked again.
        if !profile.enabled && update.enabled == Some(true) {
            if let Err(error) = self.sync_hours(&profile, clock).await {
                tracing::warn!(
                    %user_id,
                    "Switching buddies on with stored XP, logged hours could not be synced: {error}"
                );
            }
        }

        let xp = self.repository.xp(user_id, clock.today).await?;
        let total_xp = xp.tally.total_xp();
        ensure_partner_allowed(
            &update,
            profile.partner.is_some(),
            profile.lead.progress(total_xp).level,
        )?;
        let profile = if update.is_empty() {
            profile
        } else {
            self.repository
                .update_profile(user_id, &update, total_xp)
                .await?
        };

        Ok(BuddyOverview {
            profile,
            total_xp,
            today: xp.today,
        })
    }
}

/// Every species in `update` must be a buddy species: one in the battle dex, which
/// also has sprites.
fn ensure_known_species(update: &BuddyProfileUpdate) -> Result<(), BuddyError> {
    for species_id in [update.lead_species_id, update.partner_species_id]
        .into_iter()
        .flatten()
    {
        if !battle::is_known_species(species_id) {
            return Err(BuddyError::UnknownSpecies(species_id));
        }
    }
    Ok(())
}

/// The profile that a user's first update creates, if it may.
fn enrolment(update: &BuddyProfileUpdate, today: Date) -> Result<NewBuddyProfile, BuddyError> {
    let (Some(enabled), Some(lead_species_id)) = (update.enabled, update.lead_species_id) else {
        return Err(BuddyError::Invalid(
            "a new buddy profile needs enabled and leadSpeciesId".to_string(),
        ));
    };
    // A new lead has no XP yet.
    ensure_partner_allowed(update, false, BuddyProgress::from_xp(0).level)?;

    Ok(NewBuddyProfile {
        enabled,
        battle_notifications: update.battle_notifications.unwrap_or(true),
        lead_species_id,
        xp_since: today,
    })
}

/// A user without a partner can create one once their lead reaches
/// `PARTNER_UNLOCK_LEVEL`, and can only show or hide a partner they have or create.
fn ensure_partner_allowed(
    update: &BuddyProfileUpdate,
    has_partner: bool,
    lead_level: u8,
) -> Result<(), BuddyError> {
    if has_partner {
        return Ok(());
    }
    if update.partner_species_id.is_some() {
        if lead_level < PARTNER_UNLOCK_LEVEL {
            return Err(BuddyError::PartnerLocked);
        }
    } else if update.partner_visible.is_some() {
        return Err(BuddyError::NoPartner);
    }
    Ok(())
}

#[cfg(test)]
mod tests;

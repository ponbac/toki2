//! Timer buddies: enrolment, buddy settings, the XP earned from logged hours, and
//! battles against colleagues' buddies and the daily trainer.
//!
//! Logged hours are read lazily: when a user opens their profile with their buddy
//! switched on, the hours of their recent days are read from time tracking, at most
//! once per `HOURS_SYNC_INTERVAL`, and stored as XP days. Other users' levels come
//! from what was stored when they last opened theirs, so no request ever waits for
//! someone else's time tracking. Battles use the stored XP of both sides too.
//!
//! A battle is decided and stored in one step, while the challenger's battles are
//! locked, so concurrent requests cannot break the cooldown or the once-a-day rules.
//! The defender is told afterwards, in the background.

use std::sync::Arc;

use async_trait::async_trait;
use time::{Date, Duration, OffsetDateTime};
use tracing::Instrument;

use super::battle;
use crate::domain::{
    models::{
        hours_sync_due, hours_sync_window, next_colleague_battle_at, xp_days, AiBillingMonth,
        AiUsageTimeZone, BattleId, BattleNotice, BattleOpponent, BattleSide, BattleView,
        BuddyBattle, BuddyBattleSummary, BuddyClock, BuddyLeaderboard, BuddyOpponents,
        BuddyOverview, BuddyProfile, BuddyProfileUpdate, BuddyProgress, DailyTrainerStatus,
        NewBuddyProfile, UserId, PARTNER_UNLOCK_LEVEL, RECENT_BATTLE_DAYS, RECENT_BATTLE_LIMIT,
    },
    ports::{
        inbound::BuddyService,
        outbound::{BuddyNotifier, BuddyRepository, LoggedHours},
    },
    BuddyError,
};

mod battles;

pub struct BuddyServiceImpl<R, H, N> {
    repository: Arc<R>,
    logged_hours: Arc<H>,
    notifier: Arc<N>,
    time_zone: AiUsageTimeZone,
}

impl<R, H, N> BuddyServiceImpl<R, H, N> {
    /// `time_zone` defines the local days that hours and battles count on.
    pub fn new(
        repository: Arc<R>,
        logged_hours: Arc<H>,
        notifier: Arc<N>,
        time_zone: AiUsageTimeZone,
    ) -> Self {
        Self {
            repository,
            logged_hours,
            notifier,
            time_zone,
        }
    }
}

impl<R: BuddyRepository, H: LoggedHours, N: BuddyNotifier> BuddyServiceImpl<R, H, N> {
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

    /// The profile of a user who may battle: enrolled, with their buddy switched on.
    async fn battling_profile(&self, user_id: &UserId) -> Result<BuddyProfile, BuddyError> {
        let profile = self
            .repository
            .profile(user_id)
            .await?
            .ok_or(BuddyError::NotEnrolled)?;
        battles::ensure_enabled(&profile)?;
        Ok(profile)
    }

    /// Tells the defender of a stored colleague battle about it, in the background so
    /// the battle never waits for or fails with the notice.
    fn notify_defender(&self, battle: &BuddyBattle) {
        let Some(notice) = BattleNotice::for_defender(&battle.summary) else {
            return;
        };
        let notifier = self.notifier.clone();
        tokio::spawn(async move { notifier.battle_fought(&notice).await }.in_current_span());
    }
}

#[async_trait]
impl<R: BuddyRepository, H: LoggedHours, N: BuddyNotifier> BuddyService
    for BuddyServiceImpl<R, H, N>
{
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

    async fn opponents(&self, user_id: &UserId) -> Result<BuddyOpponents, BuddyError> {
        let profile = self.battling_profile(user_id).await?;
        let clock = self.repository.clock(&self.time_zone).await?;
        let rivals = self.repository.rivals(user_id, clock.today).await?;
        let last_battle_at = self.repository.last_colleague_battle_at(user_id).await?;

        Ok(BuddyOpponents {
            next_battle_at: next_colleague_battle_at(last_battle_at, clock.now),
            opponents: rivals
                .into_iter()
                .map(|rival| battles::opponent(&profile, rival))
                .collect(),
        })
    }

    async fn battle_colleague(
        &self,
        user_id: &UserId,
        opponent_id: &UserId,
    ) -> Result<BattleView<BuddyBattle>, BuddyError> {
        if user_id == opponent_id {
            return Err(BuddyError::Invalid(
                "you cannot battle your own buddies".to_string(),
            ));
        }
        let seed = rand::random::<u32>();
        let (battle, setup) = self
            .repository
            .start_battle(
                user_id,
                BattleOpponent::Colleague(*opponent_id),
                &self.time_zone,
                &|setup| battles::plan_colleague_battle(setup, seed),
            )
            .await?;

        if setup
            .defender
            .as_ref()
            .is_some_and(|defender| defender.profile.battle_notifications)
        {
            self.notify_defender(&battle);
        }
        Ok(BattleView {
            role: BattleSide::Challenger,
            battle,
        })
    }

    async fn trainer(&self, user_id: &UserId) -> Result<DailyTrainerStatus, BuddyError> {
        let profile = self.battling_profile(user_id).await?;
        let clock = self.repository.clock(&self.time_zone).await?;
        let trainer = battle::daily_trainer(clock.today);
        let fought = self.repository.trainer_battle(user_id, clock.today).await?;

        Ok(DailyTrainerStatus {
            date: clock.today,
            team: battles::trainer_team(&trainer, &profile),
            name: trainer.name,
            battle: fought,
        })
    }

    async fn battle_trainer(
        &self,
        user_id: &UserId,
    ) -> Result<BattleView<BuddyBattle>, BuddyError> {
        let seed = rand::random::<u32>();
        let (battle, _) = self
            .repository
            .start_battle(
                user_id,
                BattleOpponent::Trainer,
                &self.time_zone,
                &|setup| battles::plan_trainer_battle(setup, seed),
            )
            .await?;

        Ok(BattleView {
            role: BattleSide::Challenger,
            battle,
        })
    }

    async fn battles(
        &self,
        user_id: &UserId,
    ) -> Result<Vec<BattleView<BuddyBattleSummary>>, BuddyError> {
        let clock = self.repository.clock(&self.time_zone).await?;
        let since = clock.now - Duration::days(RECENT_BATTLE_DAYS);

        self.repository
            .recent_battles(user_id, since, RECENT_BATTLE_LIMIT)
            .await?
            .into_iter()
            .map(|battle| {
                let role = battle.role_of(*user_id).ok_or_else(|| {
                    BuddyError::Storage(format!(
                        "battle {} was listed for user {user_id}, who did not take part",
                        battle.id
                    ))
                })?;
                Ok(BattleView { role, battle })
            })
            .collect()
    }

    async fn battle(
        &self,
        user_id: &UserId,
        battle_id: BattleId,
    ) -> Result<BattleView<BuddyBattle>, BuddyError> {
        let battle = self
            .repository
            .battle(battle_id)
            .await?
            .ok_or(BuddyError::BattleNotFound)?;
        // Others do not learn that the battle exists.
        let role = battle
            .summary
            .role_of(*user_id)
            .ok_or(BuddyError::BattleNotFound)?;
        Ok(BattleView { role, battle })
    }

    async fn mark_battle_seen(
        &self,
        user_id: &UserId,
        battle_id: BattleId,
    ) -> Result<(), BuddyError> {
        let battle = self
            .repository
            .battle(battle_id)
            .await?
            .ok_or(BuddyError::BattleNotFound)?;
        match battle.summary.role_of(*user_id) {
            None => Err(BuddyError::NotParticipant),
            Some(BattleSide::Defender) if battle.summary.defender_seen_at.is_none() => {
                self.repository.mark_battle_seen(battle_id, user_id).await
            }
            // The challenger saw it start, and the first look counts.
            Some(_) => Ok(()),
        }
    }

    async fn leaderboard(
        &self,
        month: Option<AiBillingMonth>,
    ) -> Result<BuddyLeaderboard, BuddyError> {
        let month = match month {
            Some(month) => month,
            None => {
                let today = self.repository.clock(&self.time_zone).await?.today;
                AiBillingMonth::containing(today).ok_or_else(|| {
                    BuddyError::Storage(format!("today, {today}, has no calendar month"))
                })?
            }
        };
        let standings = self.repository.standings(month).await?;

        Ok(BuddyLeaderboard {
            month,
            rows: standings
                .into_iter()
                .map(battles::leaderboard_row)
                .collect(),
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

use async_trait::async_trait;
use time::{Date, OffsetDateTime};

use crate::domain::{
    models::{
        AiBillingMonth, AiUsageTimeZone, BattleId, BattleNotice, BattleOpponent, BattleSetup,
        BuddyBattle, BuddyBattleSummary, BuddyClock, BuddyProfile, BuddyProfileUpdate, BuddyRival,
        BuddyStanding, BuddyXpDay, BuddyXpSummary, DailyHours, NewBuddyBattle, NewBuddyProfile,
        TrainerBattle, UserId,
    },
    BuddyError,
};

/// Decides a new battle from its setup: checks the rules and resolves it, or refuses.
pub type BattlePlan<'a> =
    dyn Fn(&BattleSetup) -> Result<NewBuddyBattle, BuddyError> + Send + Sync + 'a;

/// Stored buddy profiles, buddies and the sources of their XP.
#[async_trait]
pub trait BuddyRepository: Send + Sync + 'static {
    /// The store's clock: now, and today's date in `time_zone`.
    async fn clock(&self, time_zone: &AiUsageTimeZone) -> Result<BuddyClock, BuddyError>;

    /// The user's profile with their buddies, or `None` before they enrol.
    async fn profile(&self, user_id: &UserId) -> Result<Option<BuddyProfile>, BuddyError>;

    /// Creates the user's profile with its lead and returns it. Returns `None`, and
    /// changes nothing, when the user already has a profile. Fails with
    /// `UserNotFound` for an unknown user.
    async fn enrol(
        &self,
        user_id: &UserId,
        profile: &NewBuddyProfile,
    ) -> Result<Option<BuddyProfile>, BuddyError>;

    /// Applies `update` to the user's profile and returns it, all or nothing. A partner
    /// species creates the partner with `new_partner_xp_base` when there is none, or
    /// else changes only its species. Fails with `NotEnrolled` without a profile, and
    /// with `NoPartner` when `partner_visible` is set but there is no partner.
    async fn update_profile(
        &self,
        user_id: &UserId,
        update: &BuddyProfileUpdate,
        new_partner_xp_base: i64,
    ) -> Result<BuddyProfile, BuddyError>;

    /// The stored sources of the user's XP, and what they earned on the local date
    /// `today`.
    async fn xp(&self, user_id: &UserId, today: Date) -> Result<BuddyXpSummary, BuddyError>;

    /// Records that a read of the user's logged hours starts now, if the last one
    /// started at `last_attempted_at`, as their profile said, and returns whether it
    /// did. Of concurrent calls with the same `last_attempted_at`, one at most claims
    /// the read; the others, and calls without a profile, change nothing.
    async fn claim_hours_sync(
        &self,
        user_id: &UserId,
        last_attempted_at: Option<OffsetDateTime>,
    ) -> Result<bool, BuddyError>;

    /// Stores the logged hours of `days`, replacing what was stored for those dates,
    /// and records that the user's hours were read now. Returns that time.
    async fn save_logged_days(
        &self,
        user_id: &UserId,
        days: &[BuddyXpDay],
    ) -> Result<OffsetDateTime, BuddyError>;

    /// Everyone but the user whose buddy is switched on, by name, with their head-to-
    /// head record against the user in colleague battles either of them started, and
    /// whether the user battled them on the local date `today`.
    async fn rivals(&self, user_id: &UserId, today: Date) -> Result<Vec<BuddyRival>, BuddyError>;

    /// When the user last started a colleague battle.
    async fn last_colleague_battle_at(
        &self,
        user_id: &UserId,
    ) -> Result<Option<OffsetDateTime>, BuddyError>;

    /// Starts a battle of `challenger_id` against `opponent`, all or nothing: locks the
    /// challenger's battles against concurrent ones, reads the setup with the clock in
    /// `time_zone`, lets `plan` decide the battle and stores it. Returns the stored
    /// battle and the setup it was decided from. Fails with what `plan` fails with,
    /// changing nothing, and with `BattledToday` or `TrainerAlreadyFought` when the
    /// challenger already had that battle today.
    async fn start_battle(
        &self,
        challenger_id: &UserId,
        opponent: BattleOpponent,
        time_zone: &AiUsageTimeZone,
        plan: &BattlePlan<'_>,
    ) -> Result<(BuddyBattle, BattleSetup), BuddyError>;

    /// The user's battle against the daily trainer on the local date `day`, if any.
    async fn trainer_battle(
        &self,
        user_id: &UserId,
        day: Date,
    ) -> Result<Option<TrainerBattle>, BuddyError>;

    /// The battles the user started or defended since `since`, newest first, at most
    /// `limit` of them.
    async fn recent_battles(
        &self,
        user_id: &UserId,
        since: OffsetDateTime,
        limit: i64,
    ) -> Result<Vec<BuddyBattleSummary>, BuddyError>;

    /// The battle, or `None` when there is no such battle.
    async fn battle(&self, battle_id: BattleId) -> Result<Option<BuddyBattle>, BuddyError>;

    /// Records that the defender saw the battle now, unless they saw it before.
    async fn mark_battle_seen(
        &self,
        battle_id: BattleId,
        defender_id: &UserId,
    ) -> Result<(), BuddyError>;

    /// Everyone who started a battle on a local date in `month`, with the results of
    /// the battles they started then.
    async fn standings(&self, month: AiBillingMonth) -> Result<Vec<BuddyStanding>, BuddyError>;
}

/// Tells users about battles that colleagues started against them.
#[async_trait]
pub trait BuddyNotifier: Send + Sync + 'static {
    /// Tells the defender how the battle went, as far as it can: failures are logged,
    /// never returned, because a battle never depends on its notice.
    async fn battle_fought(&self, notice: &BattleNotice);
}

/// The hours users logged in time tracking.
#[async_trait]
pub trait LoggedHours: Send + Sync + 'static {
    /// The hours `user` worked on each local day in `range`, both ends inclusive. Days
    /// without hours may be left out. `None` when the user is not connected to time
    /// tracking, which says nothing about the hours they worked.
    async fn daily_worked_hours(
        &self,
        user: UserId,
        range: (Date, Date),
    ) -> Result<Option<Vec<DailyHours>>, BuddyError>;
}

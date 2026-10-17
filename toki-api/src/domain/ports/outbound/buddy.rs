use async_trait::async_trait;
use time::{Date, OffsetDateTime};

use crate::domain::{
    models::{
        AiUsageTimeZone, BuddyClock, BuddyProfile, BuddyProfileUpdate, BuddyXpDay, BuddyXpSummary,
        DailyHours, NewBuddyProfile, UserId,
    },
    BuddyError,
};

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

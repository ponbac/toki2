//! Timer buddies: a user's buddy profile, their one or two buddies, and the XP and
//! levels the buddies earn.
//!
//! A user enrols by choosing a lead buddy. The partner unlocks when the lead reaches
//! `PARTNER_UNLOCK_LEVEL`; once set it can change species or be hidden, but never
//! removed. Changing a buddy's species keeps its XP and level.
//!
//! XP is earned per user, not per buddy: a user's total XP is the XP of each local day
//! with logged hours since they enrolled (`logged_hours_xp`), plus `XP_PER_BATTLE` for
//! each battle they started. The lead's XP is the total; the partner's is what the user
//! earned after it was first set, the total above its `xp_base`.
//!
//! Days are local calendar dates in the `ai_usage.time_zone` setting.

use time::{Date, Duration, OffsetDateTime, UtcOffset};

use super::{DailyHours, SpeciesId, UserId, MAX_BUDDY_LEVEL};

/// The lead level at which the partner slot unlocks.
pub const PARTNER_UNLOCK_LEVEL: u8 = 10;
/// XP per hour logged on a day.
pub const XP_PER_LOGGED_HOUR: f64 = 60.0;
/// Hours logged on one day beyond this give no more XP.
pub const MAX_XP_HOURS_PER_DAY: f64 = 10.0;
/// XP for each battle a user starts, against a colleague or the daily trainer, whatever
/// its result. Defending gives none.
pub const XP_PER_BATTLE: i64 = 20;
/// Logged hours are read again for this many days before today, so that hours logged
/// or edited later still count.
pub const HOURS_SYNC_DAYS: i64 = 31;
/// Logged hours on days since the last read are read up to this many days before
/// today, so hours logged while the user was away still count.
pub const MAX_HOURS_SYNC_DAYS: i64 = 92;
/// Logged hours are read from time tracking at most this often per user, whether or
/// not the last read worked.
pub const HOURS_SYNC_INTERVAL: Duration = Duration::seconds(60);

/// The total XP a buddy needs to reach `level`: `ceil(0.7 × (level − 1)³)` in whole
/// numbers, so Lv 1 needs none, Lv 2 needs 1 and Lv 100 needs 679 210. Levels
/// outside 1..=100 are clamped.
pub fn xp_for_level(level: u8) -> i64 {
    let steps = i64::from(level.clamp(1, MAX_BUDDY_LEVEL) - 1);
    // Rounds up; the product is never negative.
    (7 * steps * steps * steps + 9) / 10
}

/// The level of a buddy with `xp`: the highest level whose `xp_for_level` it has
/// reached. A buddy without XP is Lv 1.
pub fn level_for_xp(xp: i64) -> u8 {
    (1..=MAX_BUDDY_LEVEL)
        .rev()
        .find(|&level| xp_for_level(level) <= xp)
        .unwrap_or(1)
}

/// XP for one local day with `worked_hours` logged: `XP_PER_LOGGED_HOUR` per hour, for
/// up to `MAX_XP_HOURS_PER_DAY` hours, rounded to whole XP.
pub fn logged_hours_xp(worked_hours: f64) -> i32 {
    let hours = valid_hours(worked_hours).min(MAX_XP_HOURS_PER_DAY);
    (hours * XP_PER_LOGGED_HOUR).round() as i32
}

/// Hours as stored: a provider's negative or non-finite hours count as none.
fn valid_hours(hours: f64) -> f64 {
    if hours.is_finite() {
        hours.max(0.0)
    } else {
        0.0
    }
}

/// A buddy's slot. Stored as 1 for the lead and 2 for the partner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BuddySlot {
    /// The first buddy, chosen on enrolment. It always fights.
    Lead,
    /// The second buddy, unlocked at `PARTNER_UNLOCK_LEVEL`.
    Partner,
}

impl BuddySlot {
    pub fn number(self) -> i16 {
        match self {
            Self::Lead => 1,
            Self::Partner => 2,
        }
    }

    pub fn from_number(number: i16) -> Option<Self> {
        match number {
            1 => Some(Self::Lead),
            2 => Some(Self::Partner),
            _ => None,
        }
    }
}

/// One of a user's buddies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Buddy {
    pub slot: BuddySlot,
    pub species_id: SpeciesId,
    /// A hidden partner is not shown, but it still fights.
    pub visible: bool,
    /// The user's total XP when the buddy was first set; it earns what the user earns
    /// after that. Always 0 for the lead.
    pub xp_base: i64,
}

impl Buddy {
    /// The buddy's own XP when its user has `total_xp`.
    pub fn xp(&self, total_xp: i64) -> i64 {
        total_xp.saturating_sub(self.xp_base).max(0)
    }

    /// The buddy's level and XP when its user has `total_xp`.
    pub fn progress(&self, total_xp: i64) -> BuddyProgress {
        BuddyProgress::from_xp(self.xp(total_xp))
    }
}

/// A buddy's XP and level, with the XP bounds of that level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuddyProgress {
    pub xp: i64,
    pub level: u8,
    /// The XP at which `level` starts.
    pub level_xp: i64,
    /// The XP at which the next level starts, `None` at the highest level.
    pub next_level_xp: Option<i64>,
}

impl BuddyProgress {
    pub fn from_xp(xp: i64) -> Self {
        let xp = xp.max(0);
        let level = level_for_xp(xp);
        Self {
            xp,
            level,
            level_xp: xp_for_level(level),
            next_level_xp: (level < MAX_BUDDY_LEVEL).then(|| xp_for_level(level + 1)),
        }
    }
}

/// A user's buddy profile, which exists from their enrolment on.
#[derive(Debug, Clone, PartialEq)]
pub struct BuddyProfile {
    pub user_id: UserId,
    /// Whether the buddy is shown and can battle and be battled.
    pub enabled: bool,
    /// Whether the user gets a push notification when a colleague battles them.
    pub battle_notifications: bool,
    /// The local date of enrolment. Logged hours count from this day on.
    pub xp_since: Date,
    /// When logged hours were last read from time tracking, `None` before the first
    /// read.
    pub hours_synced_at: Option<OffsetDateTime>,
    /// When a read of logged hours last started, whether or not it worked, `None`
    /// before the first.
    pub hours_sync_attempted_at: Option<OffsetDateTime>,
    pub lead: Buddy,
    pub partner: Option<Buddy>,
}

impl BuddyProfile {
    /// The buddies in slot order: the lead, then the partner if there is one.
    pub fn buddies(&self) -> impl Iterator<Item = &Buddy> {
        std::iter::once(&self.lead).chain(self.partner.as_ref())
    }
}

/// What a user chooses on enrolment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NewBuddyProfile {
    pub enabled: bool,
    pub battle_notifications: bool,
    pub lead_species_id: SpeciesId,
    /// Today's local date.
    pub xp_since: Date,
}

/// Changes a user asks for to their buddy profile; `None` leaves a setting as it is.
/// The first update enrols the user and needs `enabled` and `lead_species_id`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BuddyProfileUpdate {
    pub enabled: Option<bool>,
    pub battle_notifications: Option<bool>,
    pub lead_species_id: Option<SpeciesId>,
    /// Sets the partner's species, creating the partner if the user has none yet.
    pub partner_species_id: Option<SpeciesId>,
    /// Shows or hides the partner, which must exist or be created by the same update.
    pub partner_visible: Option<bool>,
}

impl BuddyProfileUpdate {
    /// Whether the update changes nothing.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// The stored sources of a user's XP.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BuddyXpTally {
    /// The XP of all the user's days with logged hours.
    pub hours_xp: i64,
    /// The battles the user started.
    pub battles: i64,
}

impl BuddyXpTally {
    /// The user's total XP, which is also their lead's XP.
    pub fn total_xp(&self) -> i64 {
        self.hours_xp
            .saturating_add(XP_PER_BATTLE.saturating_mul(self.battles))
    }
}

/// The XP a user earned on one local day.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuddyDay {
    pub date: Date,
    /// The hours logged that day, as last read from time tracking.
    pub logged_hours: f64,
    /// The XP those hours give.
    pub hours_xp: i64,
    /// The battles the user started that day.
    pub battles: i64,
}

impl BuddyDay {
    /// The XP the day's battles give.
    pub fn battle_xp(&self) -> i64 {
        XP_PER_BATTLE.saturating_mul(self.battles)
    }
}

/// A user's XP in total and on today's local date.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuddyXpSummary {
    pub tally: BuddyXpTally,
    pub today: BuddyDay,
}

/// A user's buddy profile with the XP their buddies have earned, as of today.
#[derive(Debug, Clone, PartialEq)]
pub struct BuddyOverview {
    pub profile: BuddyProfile,
    pub total_xp: i64,
    pub today: BuddyDay,
}

impl BuddyOverview {
    /// The level and XP of one of the profile's buddies.
    pub fn progress(&self, buddy: &Buddy) -> BuddyProgress {
        buddy.progress(self.total_xp)
    }
}

/// The store's clock: now, and today's local date.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuddyClock {
    pub now: OffsetDateTime,
    pub today: Date,
}

/// The hours a user logged on one local day, and the XP they give.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BuddyXpDay {
    pub date: Date,
    pub logged_hours: f64,
    pub xp: i32,
}

impl BuddyXpDay {
    pub fn new(date: Date, logged_hours: f64) -> Self {
        let logged_hours = valid_hours(logged_hours);
        Self {
            date,
            logged_hours,
            xp: logged_hours_xp(logged_hours),
        }
    }
}

/// Whether logged hours are due to be read again: no read was started yet, or none
/// within the last `HOURS_SYNC_INTERVAL`, whether or not it worked. A read stamped in
/// the future, after the clock went back, does not hold off the next one.
pub fn hours_sync_due(attempted_at: Option<OffsetDateTime>, now: OffsetDateTime) -> bool {
    attempted_at.is_none_or(|attempted_at| {
        let since_attempt = now - attempted_at;
        !(Duration::ZERO..HOURS_SYNC_INTERVAL).contains(&since_attempt)
    })
}

/// The local days whose logged hours a sync reads, both inclusive, through today: the
/// last `HOURS_SYNC_DAYS` before today, and further back every day since the last
/// read at `synced_at`, or since enrolment before the first read, but never more than
/// `MAX_HOURS_SYNC_DAYS` before today, nor before enrolment. `None` when the user
/// enrolled after today.
pub fn hours_sync_window(
    xp_since: Date,
    today: Date,
    synced_at: Option<OffsetDateTime>,
) -> Option<(Date, Date)> {
    let days_before_today = |days| today.checked_sub(Duration::days(days)).unwrap_or(Date::MIN);
    // The day before the UTC date of the last read is never after its local date.
    let unread_since = synced_at.map_or(xp_since, |synced_at| {
        let utc_date = synced_at.to_offset(UtcOffset::UTC).date();
        utc_date.previous_day().unwrap_or(Date::MIN)
    });
    let earliest = days_before_today(HOURS_SYNC_DAYS)
        .min(unread_since)
        .max(days_before_today(MAX_HOURS_SYNC_DAYS));
    let from = xp_since.max(earliest);
    (from <= today).then_some((from, today))
}

/// One XP day for each date in `window`, both inclusive, in date order: the hours
/// logged that day, or none. Hours on the same date are added up, and hours outside
/// the window are left out.
pub fn xp_days(window: (Date, Date), hours: &[DailyHours]) -> Vec<BuddyXpDay> {
    let (from, to) = window;
    let mut days = Vec::new();
    let mut date = from;
    while date <= to {
        let logged_hours = hours
            .iter()
            .filter(|day| day.date == date)
            .map(|day| valid_hours(day.worked_hours))
            .sum();
        days.push(BuddyXpDay::new(date, logged_hours));
        match date.next_day() {
            Some(next) => date = next,
            None => break,
        }
    }
    days
}

#[cfg(test)]
mod tests {
    use time::macros::{date, datetime};

    use super::*;

    #[test]
    fn levels_follow_the_cubic_curve() {
        for (level, xp) in [
            (1, 0),
            (2, 1),
            (3, 6),
            (10, 511),
            (11, 700),
            (16, 2_363),
            (26, 10_938),
            (57, 122_932),
            // Exact, where a floating-point 0.7 would come out one XP short.
            (71, 240_100),
            (91, 510_300),
            (100, 679_210),
        ] {
            assert_eq!(xp_for_level(level), xp, "Lv {level}");
        }
    }

    #[test]
    fn a_level_starts_exactly_at_its_xp() {
        for (xp, level) in [
            (i64::MIN, 1),
            (-1, 1),
            (0, 1),
            (1, 2),
            (5, 2),
            (6, 3),
            (510, 9),
            (511, 10),
            (699, 10),
            (700, 11),
            (240_099, 70),
            (240_100, 71),
            (679_209, 99),
            (679_210, 100),
            (i64::MAX, 100),
        ] {
            assert_eq!(level_for_xp(xp), level, "{xp} XP");
        }
        for level in 1..=MAX_BUDDY_LEVEL {
            assert_eq!(level_for_xp(xp_for_level(level)), level);
            if level > 1 {
                assert_eq!(level_for_xp(xp_for_level(level) - 1), level - 1);
            }
        }
    }

    #[test]
    fn one_full_working_day_and_three_battles_reach_the_partner_unlock() {
        let tally = BuddyXpTally {
            hours_xp: i64::from(logged_hours_xp(8.0)),
            battles: 3,
        };

        assert_eq!(tally.total_xp(), 540);
        assert_eq!(level_for_xp(tally.total_xp()), PARTNER_UNLOCK_LEVEL);
        assert!(level_for_xp(i64::from(logged_hours_xp(8.0))) < PARTNER_UNLOCK_LEVEL);
    }

    #[test]
    fn logged_hours_give_whole_xp_for_at_most_ten_hours_a_day() {
        for (hours, xp) in [
            (0.0, 0),
            (0.5, 30),
            (1.0 / 60.0, 1),
            (0.0083, 0),
            (0.0084, 1),
            (6.5, 390),
            (10.0, 600),
            (14.25, 600),
            (-2.0, 0),
            (f64::NAN, 0),
            (f64::INFINITY, 0),
        ] {
            assert_eq!(logged_hours_xp(hours), xp, "{hours} h");
        }
    }

    #[test]
    fn progress_reports_the_bounds_of_the_current_level() {
        assert_eq!(
            BuddyProgress::from_xp(600),
            BuddyProgress {
                xp: 600,
                level: 10,
                level_xp: 511,
                next_level_xp: Some(700),
            }
        );
        assert_eq!(
            BuddyProgress::from_xp(900_000),
            BuddyProgress {
                xp: 900_000,
                level: 100,
                level_xp: 679_210,
                next_level_xp: None,
            }
        );
    }

    #[test]
    fn a_partner_earns_only_the_xp_after_it_was_set() {
        let partner = Buddy {
            slot: BuddySlot::Partner,
            species_id: SpeciesId::new(25),
            visible: true,
            xp_base: 540,
        };

        assert_eq!(partner.xp(540), 0);
        assert_eq!(partner.xp(1_000), 460);
        // Hours can be edited away after the partner was set.
        assert_eq!(partner.xp(500), 0);
    }

    #[test]
    fn hours_are_read_again_once_a_minute_has_passed_since_the_last_attempt() {
        let now = datetime!(2026-10-16 12:00 UTC);

        assert!(hours_sync_due(None, now));
        assert!(!hours_sync_due(Some(now), now));
        assert!(!hours_sync_due(Some(now - Duration::seconds(59)), now));
        assert!(hours_sync_due(Some(now - Duration::seconds(60)), now));
        assert!(hours_sync_due(Some(now + Duration::hours(1)), now));
    }

    #[test]
    fn the_sync_window_starts_at_enrolment_or_31_days_back() {
        let today = date!(2026 - 10 - 16);
        let synced_at = Some(datetime!(2026-10-16 08:00 UTC));

        assert_eq!(
            hours_sync_window(today, today, synced_at),
            Some((today, today))
        );
        assert_eq!(
            hours_sync_window(date!(2026 - 10 - 01), today, synced_at),
            Some((date!(2026 - 10 - 01), today))
        );
        assert_eq!(
            hours_sync_window(date!(2026 - 01 - 01), today, synced_at),
            Some((date!(2026 - 09 - 15), today))
        );
        assert_eq!(
            hours_sync_window(date!(2026 - 10 - 17), today, synced_at),
            None
        );
        // A read stamped in the future, after the clock went back, reads as usual.
        assert_eq!(
            hours_sync_window(
                date!(2026 - 01 - 01),
                today,
                Some(datetime!(2026-12-01 08:00 UTC))
            ),
            Some((date!(2026 - 09 - 15), today))
        );
    }

    #[test]
    fn the_sync_window_reaches_back_to_days_no_read_saw_for_up_to_92_days() {
        let today = date!(2026 - 10 - 16);
        let enrolled = date!(2026 - 01 - 01);

        // The last read, 45 days ago, may have been on the next local day.
        assert_eq!(
            hours_sync_window(enrolled, today, Some(datetime!(2026-09-01 23:30 UTC))),
            Some((date!(2026 - 08 - 31), today))
        );
        assert_eq!(
            hours_sync_window(enrolled, today, Some(datetime!(2026-03-01 08:00 UTC))),
            Some((date!(2026 - 07 - 16), today))
        );
        // Before the first read, every day since enrolment is unread.
        assert_eq!(
            hours_sync_window(date!(2026 - 08 - 01), today, None),
            Some((date!(2026 - 08 - 01), today))
        );
        assert_eq!(
            hours_sync_window(enrolled, today, None),
            Some((date!(2026 - 07 - 16), today))
        );
        assert_eq!(
            hours_sync_window(date!(2026 - 10 - 10), today, None),
            Some((date!(2026 - 10 - 10), today))
        );
    }

    #[test]
    fn every_day_in_the_window_gets_its_hours_or_none() {
        let hours = [
            DailyHours {
                date: date!(2026 - 10 - 14),
                worked_hours: 3.0,
            },
            DailyHours {
                date: date!(2026 - 10 - 14),
                worked_hours: 1.5,
            },
            DailyHours {
                date: date!(2026 - 10 - 16),
                worked_hours: 12.0,
            },
            DailyHours {
                date: date!(2026 - 10 - 10),
                worked_hours: 8.0,
            },
        ];

        assert_eq!(
            xp_days((date!(2026 - 10 - 14), date!(2026 - 10 - 16)), &hours),
            vec![
                BuddyXpDay {
                    date: date!(2026 - 10 - 14),
                    logged_hours: 4.5,
                    xp: 270,
                },
                BuddyXpDay {
                    date: date!(2026 - 10 - 15),
                    logged_hours: 0.0,
                    xp: 0,
                },
                BuddyXpDay {
                    date: date!(2026 - 10 - 16),
                    logged_hours: 12.0,
                    xp: 600,
                },
            ]
        );
    }
}

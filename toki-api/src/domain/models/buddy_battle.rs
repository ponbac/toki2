//! Timer buddy battles as users meet them: the colleagues they can battle, the daily
//! trainer, the battles they fought and the monthly leaderboard.
//!
//! A user whose buddy is switched on battles a colleague whose buddy is switched on, at
//! most one colleague battle per `COLLEAGUE_BATTLE_COOLDOWN` and each colleague at most
//! once per local day, and the daily trainer once per local day, apart from that
//! cooldown. When both sides have a partner they fight two against two, lead first;
//! otherwise lead against lead. Buddies fight at the level they have when the battle
//! starts, which the stored team snapshots keep. Starting a battle earns the user
//! `XP_PER_BATTLE`, whatever the result; defending earns nothing.
//!
//! Days and months are local calendar dates in the `ai_usage.time_zone` setting.

use time::{Date, Duration, OffsetDateTime};

use super::{
    AiBillingMonth, BattleEvent, BattleId, BattleSide, BattleTeam, BuddyClock, BuddyProfile,
    BuddySlot, SpeciesId, UserId,
};

/// A user starts at most one colleague battle within this rolling window.
pub const COLLEAGUE_BATTLE_COOLDOWN: Duration = Duration::hours(1);
/// A user's battle list covers the battles of this many days...
pub const RECENT_BATTLE_DAYS: i64 = 30;
/// ...and at most this many of them, newest first.
pub const RECENT_BATTLE_LIMIT: i64 = 50;

/// What a battle was fought against.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BattleKind {
    /// A colleague's buddies.
    Colleague,
    /// The daily trainer.
    Trainer,
}

/// Whom a user battles.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleOpponent {
    /// The buddies of the colleague with this id.
    Colleague(UserId),
    /// The daily trainer of the day the battle starts.
    Trainer,
}

impl BattleOpponent {
    pub fn kind(self) -> BattleKind {
        match self {
            Self::Colleague(_) => BattleKind::Colleague,
            Self::Trainer => BattleKind::Trainer,
        }
    }
}

/// How a battle ended for one side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BattleResult {
    Win,
    Loss,
    Draw,
}

impl BattleResult {
    /// The result for `side` of a battle that `winner` won; no winner is a draw.
    pub fn of(winner: Option<BattleSide>, side: BattleSide) -> Self {
        match winner {
            None => Self::Draw,
            Some(winner) if winner == side => Self::Win,
            Some(_) => Self::Loss,
        }
    }
}

/// Battles won, lost and drawn.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BattleTally {
    pub wins: i64,
    pub losses: i64,
    pub draws: i64,
}

/// A user's buddies as they fight: their profile, their name, and the total XP that
/// sets their buddies' levels.
#[derive(Debug, Clone, PartialEq)]
pub struct BuddyCompetitor {
    pub profile: BuddyProfile,
    pub full_name: String,
    pub total_xp: i64,
}

impl BuddyCompetitor {
    pub fn user_id(&self) -> UserId {
        self.profile.user_id
    }

    /// The buddies that fight, with their current levels, in send-out order: the lead,
    /// then, if `with_partner` and the user has one, the partner. A hidden partner
    /// fights too.
    pub fn team(&self, with_partner: bool) -> Vec<(SpeciesId, u8)> {
        self.profile
            .buddies()
            .filter(|buddy| with_partner || buddy.slot == BuddySlot::Lead)
            .map(|buddy| (buddy.species_id, buddy.progress(self.total_xp).level))
            .collect()
    }
}

/// Whether a battle between two users is two against two, which needs a partner on
/// both sides. Otherwise only the leads fight.
pub fn partners_fight(challenger: &BuddyProfile, defender: &BuddyProfile) -> bool {
    challenger.partner.is_some() && defender.partner.is_some()
}

/// When a user who last started a colleague battle at `last` may start the next one,
/// or `None` when they may start one `now`.
pub fn next_colleague_battle_at(
    last: Option<OffsetDateTime>,
    now: OffsetDateTime,
) -> Option<OffsetDateTime> {
    last.map(|last| last + COLLEAGUE_BATTLE_COOLDOWN)
        .filter(|next| *next > now)
}

/// A colleague a user can battle, with their head-to-head record from the user's side,
/// counting the battles either of them started.
#[derive(Debug, Clone, PartialEq)]
pub struct BuddyRival {
    pub competitor: BuddyCompetitor,
    pub record: BattleTally,
    /// Whether the user already battled them today.
    pub battled_today: bool,
}

/// A colleague a user can battle, with the team that would fight the user.
#[derive(Debug, Clone, PartialEq)]
pub struct BuddyOpponent {
    pub user_id: UserId,
    pub full_name: String,
    /// The colleague's buddies that would fight, with their levels, lead first.
    pub team: Vec<(SpeciesId, u8)>,
    /// Head-to-head, from the user's side.
    pub record: BattleTally,
    /// Whether the user already battled them today, which rules out another battle.
    pub battled_today: bool,
}

/// The colleagues a user can battle, by name, and when they can start the next
/// colleague battle.
#[derive(Debug, Clone, PartialEq)]
pub struct BuddyOpponents {
    /// `None` when a colleague battle can start now.
    pub next_battle_at: Option<OffsetDateTime>,
    pub opponents: Vec<BuddyOpponent>,
}

/// What a new battle is decided from, read while the challenger's battles are locked
/// against concurrent ones.
#[derive(Debug, Clone, PartialEq)]
pub struct BattleSetup {
    pub clock: BuddyClock,
    /// `None` when the challenger has not enrolled.
    pub challenger: Option<BuddyCompetitor>,
    /// The colleague battled; `None` for the daily trainer, or when the colleague has
    /// not enrolled.
    pub defender: Option<BuddyCompetitor>,
    /// When the challenger last started a colleague battle.
    pub last_colleague_battle_at: Option<OffsetDateTime>,
    /// Whether the challenger already battled this colleague, or the daily trainer,
    /// today.
    pub fought_today: bool,
}

/// A resolved battle, to be stored.
#[derive(Debug, Clone, PartialEq)]
pub struct NewBuddyBattle {
    /// The local date the battle started.
    pub battle_day: Date,
    pub seed: u32,
    pub engine_version: i16,
    pub challenger_team: BattleTeam,
    pub defender_team: BattleTeam,
    /// The daily trainer's name; `None` for a colleague battle.
    pub trainer_name: Option<String>,
    pub events: Vec<BattleEvent>,
    /// `None` for a draw.
    pub winner: Option<BattleSide>,
}

/// One side of a stored battle.
#[derive(Debug, Clone, PartialEq)]
pub struct BattleParticipant {
    /// `None` for the daily trainer.
    pub user_id: Option<UserId>,
    /// The user's full name, or the daily trainer's name.
    pub name: String,
    /// The buddies as they entered the battle.
    pub team: BattleTeam,
}

/// A stored battle without its seed and events.
#[derive(Debug, Clone, PartialEq)]
pub struct BuddyBattleSummary {
    pub id: BattleId,
    pub kind: BattleKind,
    pub created_at: OffsetDateTime,
    pub challenger: BattleParticipant,
    pub defender: BattleParticipant,
    /// `None` for a draw.
    pub winner: Option<BattleSide>,
    /// When the defender first opened the battle; always `None` for the daily trainer.
    pub defender_seen_at: Option<OffsetDateTime>,
}

impl BuddyBattleSummary {
    /// The side `user_id` fought on, or `None` when they did not take part.
    pub fn role_of(&self, user_id: UserId) -> Option<BattleSide> {
        if self.challenger.user_id == Some(user_id) {
            Some(BattleSide::Challenger)
        } else if self.defender.user_id == Some(user_id) {
            Some(BattleSide::Defender)
        } else {
            None
        }
    }

    pub fn participant(&self, side: BattleSide) -> &BattleParticipant {
        match side {
            BattleSide::Challenger => &self.challenger,
            BattleSide::Defender => &self.defender,
        }
    }

    pub fn result_for(&self, side: BattleSide) -> BattleResult {
        BattleResult::of(self.winner, side)
    }

    /// Whether `side` has seen the battle. The challenger has, having started it.
    pub fn seen_by(&self, side: BattleSide) -> bool {
        match side {
            BattleSide::Challenger => true,
            BattleSide::Defender => self.defender_seen_at.is_some(),
        }
    }
}

/// A stored battle with everything a replay needs.
#[derive(Debug, Clone, PartialEq)]
pub struct BuddyBattle {
    pub summary: BuddyBattleSummary,
    pub seed: u32,
    pub engine_version: i16,
    pub events: Vec<BattleEvent>,
}

/// A battle as one of its participants sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct BattleView<B> {
    /// The side the participant fought on.
    pub role: BattleSide,
    pub battle: B,
}

/// A user's battle against the daily trainer of one day. The user is the challenger.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrainerBattle {
    pub id: BattleId,
    pub winner: Option<BattleSide>,
}

impl TrainerBattle {
    /// How the battle ended for the user.
    pub fn result(&self) -> BattleResult {
        BattleResult::of(self.winner, BattleSide::Challenger)
    }
}

/// Today's daily trainer as one user meets it.
#[derive(Debug, Clone, PartialEq)]
pub struct DailyTrainerStatus {
    /// Today's local date, which picks the trainer.
    pub date: Date,
    pub name: String,
    /// The trainer's buddies that would fight the user, with their levels.
    pub team: Vec<(SpeciesId, u8)>,
    /// The user's battle against today's trainer, if they fought it.
    pub battle: Option<TrainerBattle>,
}

/// What a user is told when a colleague battled their buddies.
#[derive(Debug, Clone, PartialEq)]
pub struct BattleNotice {
    pub battle_id: BattleId,
    pub defender_id: UserId,
    pub challenger_name: String,
    /// The species names of the defender's buddies that fought, in send-out order.
    pub defender_buddies: Vec<String>,
    /// How the battle ended for the defender.
    pub result: BattleResult,
}

impl BattleNotice {
    /// The notice for the defender of `battle`; `None` for a battle against the daily
    /// trainer, who has no one to tell.
    pub fn for_defender(battle: &BuddyBattleSummary) -> Option<Self> {
        Some(Self {
            battle_id: battle.id,
            defender_id: battle.defender.user_id?,
            challenger_name: battle.challenger.name.clone(),
            defender_buddies: battle
                .defender
                .team
                .members()
                .iter()
                .map(|mon| mon.name.clone())
                .collect(),
            result: battle.result_for(BattleSide::Defender),
        })
    }
}

/// The battles a user started in one local month.
#[derive(Debug, Clone, PartialEq)]
pub struct BuddyStanding {
    pub competitor: BuddyCompetitor,
    /// Colleague battles only.
    pub colleague: BattleTally,
    /// The days the user beat the daily trainer.
    pub trainer_wins: i64,
}

/// One user's row on a monthly leaderboard.
#[derive(Debug, Clone, PartialEq)]
pub struct BuddyLeaderboardRow {
    pub user_id: UserId,
    pub full_name: String,
    /// All of the user's buddies with their current levels, lead first.
    pub team: Vec<(SpeciesId, u8)>,
    /// Colleague battles the user started.
    pub colleague: BattleTally,
    /// The days the user beat the daily trainer.
    pub trainer_wins: i64,
}

/// Everyone who started a battle in a local month, in no particular order.
#[derive(Debug, Clone, PartialEq)]
pub struct BuddyLeaderboard {
    pub month: AiBillingMonth,
    pub rows: Vec<BuddyLeaderboardRow>,
}

#[cfg(test)]
mod tests {
    use time::macros::{date, datetime};

    use super::*;
    use crate::domain::models::Buddy;

    fn competitor(partner: Option<SpeciesId>, total_xp: i64) -> BuddyCompetitor {
        BuddyCompetitor {
            profile: BuddyProfile {
                user_id: UserId::new(7),
                enabled: true,
                battle_notifications: true,
                xp_since: date!(2026 - 10 - 01),
                hours_synced_at: None,
                hours_sync_attempted_at: None,
                lead: Buddy {
                    slot: BuddySlot::Lead,
                    species_id: SpeciesId::new(25),
                    visible: true,
                    xp_base: 0,
                },
                partner: partner.map(|species_id| Buddy {
                    slot: BuddySlot::Partner,
                    species_id,
                    visible: false,
                    xp_base: 540,
                }),
            },
            full_name: "Anna Lind".to_string(),
            total_xp,
        }
    }

    #[test]
    fn a_result_is_seen_from_one_side() {
        use BattleSide::{Challenger, Defender};

        assert_eq!(
            BattleResult::of(Some(Challenger), Challenger),
            BattleResult::Win
        );
        assert_eq!(
            BattleResult::of(Some(Challenger), Defender),
            BattleResult::Loss
        );
        assert_eq!(
            BattleResult::of(Some(Defender), Defender),
            BattleResult::Win
        );
        assert_eq!(BattleResult::of(None, Challenger), BattleResult::Draw);
        assert_eq!(BattleResult::of(None, Defender), BattleResult::Draw);
    }

    #[test]
    fn a_hidden_partner_fights_at_its_own_level_only_in_pairs() {
        let pair = competitor(Some(SpeciesId::new(1)), 1_240);

        // 1 240 XP is Lv 13 for the lead; the partner's 700 XP above its base is
        // Lv 11.
        assert_eq!(
            pair.team(true),
            vec![(SpeciesId::new(25), 13), (SpeciesId::new(1), 11)]
        );
        assert_eq!(pair.team(false), vec![(SpeciesId::new(25), 13)]);
        assert_eq!(
            competitor(None, 1_240).team(true),
            vec![(SpeciesId::new(25), 13)]
        );
    }

    #[test]
    fn partners_fight_only_when_both_sides_have_one() {
        let pair = competitor(Some(SpeciesId::new(1)), 0);
        let single = competitor(None, 0);

        assert!(partners_fight(&pair.profile, &pair.profile));
        assert!(!partners_fight(&pair.profile, &single.profile));
        assert!(!partners_fight(&single.profile, &pair.profile));
    }

    #[test]
    fn the_next_colleague_battle_is_an_hour_after_the_last() {
        let now = datetime!(2026-10-16 12:00 UTC);

        assert_eq!(next_colleague_battle_at(None, now), None);
        assert_eq!(
            next_colleague_battle_at(Some(datetime!(2026-10-16 11:15 UTC)), now),
            Some(datetime!(2026-10-16 12:15 UTC))
        );
        assert_eq!(
            next_colleague_battle_at(Some(datetime!(2026-10-16 11:00 UTC)), now),
            None
        );
        // A clock that went back still waits out the hour from the last battle.
        assert_eq!(
            next_colleague_battle_at(Some(datetime!(2026-10-16 12:30 UTC)), now),
            Some(datetime!(2026-10-16 13:30 UTC))
        );
    }
}

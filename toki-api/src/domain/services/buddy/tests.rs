use std::{
    collections::{BTreeMap, HashMap},
    sync::{Mutex, MutexGuard},
};

use time::{
    macros::{date, datetime},
    Duration,
};
use tokio::sync::mpsc;

use super::*;
use crate::domain::models::{
    BattleKind, BattleParticipant, BattleResult, BattleSetup, BattleTally, Buddy,
    BuddyBattleSummary, BuddyCompetitor, BuddyDay, BuddyRival, BuddySlot, BuddyStanding,
    BuddyXpDay, BuddyXpSummary, BuddyXpTally, DailyHours, SpeciesId, TrainerBattle,
};
use crate::domain::ports::outbound::BattlePlan;

mod battles;

const USER: UserId = UserId::new(7);
const ANNA: UserId = UserId::new(8);
const BERTIL: UserId = UserId::new(9);
const PIKACHU: SpeciesId = SpeciesId::new(25);
const BULBASAUR: SpeciesId = SpeciesId::new(1);
const CHARMANDER: SpeciesId = SpeciesId::new(4);

/// A stored battle with what the rules look it up by.
struct MemoryBattle {
    battle: BuddyBattle,
    opponent: BattleOpponent,
    day: Date,
}

impl MemoryBattle {
    fn challenger_id(&self) -> UserId {
        self.battle
            .summary
            .challenger
            .user_id
            .expect("challengers are users")
    }
}

/// Several users' stored buddy data, behind one lock like a serialising database.
struct Store {
    clock: BuddyClock,
    names: HashMap<UserId, String>,
    profiles: HashMap<UserId, BuddyProfile>,
    days: HashMap<UserId, BTreeMap<Date, BuddyXpDay>>,
    battles: Vec<MemoryBattle>,
}

impl Store {
    fn tally(&self, user_id: UserId) -> BuddyXpTally {
        BuddyXpTally {
            hours_xp: self
                .days
                .get(&user_id)
                .map_or(0, |days| days.values().map(|day| i64::from(day.xp)).sum()),
            battles: self
                .battles
                .iter()
                .filter(|battle| battle.challenger_id() == user_id)
                .count() as i64,
        }
    }

    fn competitor(&self, user_id: UserId) -> Option<BuddyCompetitor> {
        Some(BuddyCompetitor {
            profile: self.profiles.get(&user_id)?.clone(),
            full_name: self.names[&user_id].clone(),
            total_xp: self.tally(user_id).total_xp(),
        })
    }

    /// Enrolled users in name order, like the store sorts them.
    fn competitors(&self, mut keep: impl FnMut(&BuddyProfile) -> bool) -> Vec<BuddyCompetitor> {
        let mut competitors: Vec<_> = self
            .profiles
            .values()
            .filter(|profile| keep(profile))
            .filter_map(|profile| self.competitor(profile.user_id))
            .collect();
        competitors.sort_by(|a, b| a.full_name.cmp(&b.full_name));
        competitors
    }
}

/// Stored buddy data in memory. `USER`, `ANNA` and `BERTIL` exist as users.
struct MemoryBuddies {
    store: Mutex<Store>,
    fail_saves: bool,
}

impl MemoryBuddies {
    fn new() -> Arc<Self> {
        Self::with_failing_saves(false)
    }

    fn with_failing_saves(fail_saves: bool) -> Arc<Self> {
        Arc::new(Self {
            store: Mutex::new(Store {
                clock: BuddyClock {
                    now: datetime!(2026-10-16 10:00 UTC),
                    today: date!(2026 - 10 - 16),
                },
                names: HashMap::from([
                    (USER, "Ada Lovelace".to_string()),
                    (ANNA, "Anna Lind".to_string()),
                    (BERTIL, "Bertil Ek".to_string()),
                ]),
                profiles: HashMap::new(),
                days: HashMap::new(),
                battles: Vec::new(),
            }),
            fail_saves,
        })
    }

    fn store(&self) -> MutexGuard<'_, Store> {
        self.store.lock().unwrap()
    }

    fn profile_now(&self) -> Option<BuddyProfile> {
        self.store().profiles.get(&USER).cloned()
    }

    /// Moves the clock on; today is the UTC date of the new time.
    fn advance(&self, by: Duration) {
        let mut store = self.store();
        store.clock.now += by;
        store.clock.today = store.clock.now.date();
    }

    fn set_hours_xp(&self, date: Date, xp: i32) {
        self.set_hours_xp_of(USER, date, xp);
    }

    fn set_hours_xp_of(&self, user_id: UserId, date: Date, xp: i32) {
        self.store().days.entry(user_id).or_default().insert(
            date,
            BuddyXpDay {
                date,
                logged_hours: f64::from(xp) / 60.0,
                xp,
            },
        );
    }

    fn battle_count(&self) -> usize {
        self.store().battles.len()
    }
}

#[async_trait]
impl BuddyRepository for MemoryBuddies {
    async fn clock(&self, _: &AiUsageTimeZone) -> Result<BuddyClock, BuddyError> {
        Ok(self.store().clock)
    }

    async fn profile(&self, user_id: &UserId) -> Result<Option<BuddyProfile>, BuddyError> {
        Ok(self.store().profiles.get(user_id).cloned())
    }

    async fn enrol(
        &self,
        user_id: &UserId,
        new_profile: &NewBuddyProfile,
    ) -> Result<Option<BuddyProfile>, BuddyError> {
        let mut store = self.store();
        if !store.names.contains_key(user_id) {
            return Err(BuddyError::UserNotFound);
        }
        if store.profiles.contains_key(user_id) {
            return Ok(None);
        }
        let profile = BuddyProfile {
            user_id: *user_id,
            enabled: new_profile.enabled,
            battle_notifications: new_profile.battle_notifications,
            xp_since: new_profile.xp_since,
            hours_synced_at: None,
            hours_sync_attempted_at: None,
            lead: Buddy {
                slot: BuddySlot::Lead,
                species_id: new_profile.lead_species_id,
                visible: true,
                xp_base: 0,
            },
            partner: None,
        };
        store.profiles.insert(*user_id, profile.clone());
        Ok(Some(profile))
    }

    async fn update_profile(
        &self,
        user_id: &UserId,
        update: &BuddyProfileUpdate,
        new_partner_xp_base: i64,
    ) -> Result<BuddyProfile, BuddyError> {
        let mut store = self.store();
        let mut profile = store
            .profiles
            .get(user_id)
            .cloned()
            .ok_or(BuddyError::NotEnrolled)?;
        profile.enabled = update.enabled.unwrap_or(profile.enabled);
        profile.battle_notifications = update
            .battle_notifications
            .unwrap_or(profile.battle_notifications);
        if let Some(species_id) = update.lead_species_id {
            profile.lead.species_id = species_id;
        }
        if let Some(species_id) = update.partner_species_id {
            let partner = profile.partner.get_or_insert(Buddy {
                slot: BuddySlot::Partner,
                species_id,
                visible: true,
                xp_base: new_partner_xp_base,
            });
            partner.species_id = species_id;
        }
        if let Some(visible) = update.partner_visible {
            profile
                .partner
                .as_mut()
                .ok_or(BuddyError::NoPartner)?
                .visible = visible;
        }
        store.profiles.insert(*user_id, profile.clone());
        Ok(profile)
    }

    async fn xp(&self, user_id: &UserId, today: Date) -> Result<BuddyXpSummary, BuddyError> {
        let store = self.store();
        let today_day = store.days.get(user_id).and_then(|days| days.get(&today));
        Ok(BuddyXpSummary {
            tally: store.tally(*user_id),
            today: BuddyDay {
                date: today,
                logged_hours: today_day.map_or(0.0, |day| day.logged_hours),
                hours_xp: today_day.map_or(0, |day| i64::from(day.xp)),
                battles: store
                    .battles
                    .iter()
                    .filter(|battle| battle.challenger_id() == *user_id && battle.day == today)
                    .count() as i64,
            },
        })
    }

    async fn claim_hours_sync(
        &self,
        user_id: &UserId,
        last_attempted_at: Option<OffsetDateTime>,
    ) -> Result<bool, BuddyError> {
        let mut store = self.store();
        let now = store.clock.now;
        Ok(match store.profiles.get_mut(user_id) {
            Some(profile) if profile.hours_sync_attempted_at == last_attempted_at => {
                profile.hours_sync_attempted_at = Some(now);
                true
            }
            _ => false,
        })
    }

    async fn save_logged_days(
        &self,
        user_id: &UserId,
        days: &[BuddyXpDay],
    ) -> Result<OffsetDateTime, BuddyError> {
        if self.fail_saves {
            return Err(BuddyError::Storage("synthetic failure".to_string()));
        }
        let mut store = self.store();
        let now = store.clock.now;
        let profile = store
            .profiles
            .get_mut(user_id)
            .ok_or(BuddyError::NotEnrolled)?;
        profile.hours_synced_at = Some(now);
        store
            .days
            .entry(*user_id)
            .or_default()
            .extend(days.iter().map(|day| (day.date, *day)));
        Ok(now)
    }

    async fn rivals(&self, user_id: &UserId, today: Date) -> Result<Vec<BuddyRival>, BuddyError> {
        let store = self.store();
        let rivals = store
            .competitors(|profile| profile.enabled && profile.user_id != *user_id)
            .into_iter()
            .map(|competitor| {
                let mut record = BattleTally::default();
                let mut battled_today = false;
                for stored in &store.battles {
                    let summary = &stored.battle.summary;
                    let Some(role) = summary.role_of(*user_id) else {
                        continue;
                    };
                    if summary.participant(role.opponent()).user_id != Some(competitor.user_id()) {
                        continue;
                    }
                    match summary.result_for(role) {
                        BattleResult::Win => record.wins += 1,
                        BattleResult::Loss => record.losses += 1,
                        BattleResult::Draw => record.draws += 1,
                    }
                    battled_today |= role == BattleSide::Challenger && stored.day == today;
                }
                BuddyRival {
                    competitor,
                    record,
                    battled_today,
                }
            })
            .collect();
        Ok(rivals)
    }

    async fn last_colleague_battle_at(
        &self,
        user_id: &UserId,
    ) -> Result<Option<OffsetDateTime>, BuddyError> {
        Ok(self
            .store()
            .battles
            .iter()
            .filter(|battle| {
                battle.challenger_id() == *user_id
                    && battle.battle.summary.kind == BattleKind::Colleague
            })
            .map(|battle| battle.battle.summary.created_at)
            .max())
    }

    async fn start_battle(
        &self,
        challenger_id: &UserId,
        opponent: BattleOpponent,
        _: &AiUsageTimeZone,
        plan: &BattlePlan<'_>,
    ) -> Result<(BuddyBattle, BattleSetup), BuddyError> {
        let mut store = self.store();
        let clock = store.clock;
        let started = |battle: &&MemoryBattle| battle.challenger_id() == *challenger_id;
        let setup = BattleSetup {
            clock,
            challenger: store.competitor(*challenger_id),
            defender: match opponent {
                BattleOpponent::Colleague(defender_id) => store.competitor(defender_id),
                BattleOpponent::Trainer => None,
            },
            last_colleague_battle_at: store
                .battles
                .iter()
                .filter(started)
                .filter(|battle| battle.battle.summary.kind == BattleKind::Colleague)
                .map(|battle| battle.battle.summary.created_at)
                .max(),
            fought_today: store
                .battles
                .iter()
                .filter(started)
                .any(|battle| battle.opponent == opponent && battle.day == clock.today),
        };

        let new_battle = plan(&setup)?;
        let (defender_id, defender_name) = match opponent {
            BattleOpponent::Colleague(defender_id) => {
                (Some(defender_id), store.names[&defender_id].clone())
            }
            BattleOpponent::Trainer => (
                None,
                new_battle
                    .trainer_name
                    .clone()
                    .expect("trainer battles name the trainer"),
            ),
        };
        let battle = BuddyBattle {
            summary: BuddyBattleSummary {
                id: BattleId::new(store.battles.len() as i32 + 1),
                kind: opponent.kind(),
                created_at: clock.now,
                challenger: BattleParticipant {
                    user_id: Some(*challenger_id),
                    name: store.names[challenger_id].clone(),
                    team: new_battle.challenger_team,
                },
                defender: BattleParticipant {
                    user_id: defender_id,
                    name: defender_name,
                    team: new_battle.defender_team,
                },
                winner: new_battle.winner,
                defender_seen_at: None,
            },
            seed: new_battle.seed,
            engine_version: new_battle.engine_version,
            events: new_battle.events,
        };
        store.battles.push(MemoryBattle {
            battle: battle.clone(),
            opponent,
            day: new_battle.battle_day,
        });
        Ok((battle, setup))
    }

    async fn trainer_battle(
        &self,
        user_id: &UserId,
        day: Date,
    ) -> Result<Option<TrainerBattle>, BuddyError> {
        Ok(self
            .store()
            .battles
            .iter()
            .find(|battle| {
                battle.challenger_id() == *user_id
                    && battle.opponent == BattleOpponent::Trainer
                    && battle.day == day
            })
            .map(|battle| TrainerBattle {
                id: battle.battle.summary.id,
                winner: battle.battle.summary.winner,
            }))
    }

    async fn recent_battles(
        &self,
        user_id: &UserId,
        since: OffsetDateTime,
        limit: i64,
    ) -> Result<Vec<BuddyBattleSummary>, BuddyError> {
        let mut battles: Vec<_> = self
            .store()
            .battles
            .iter()
            .map(|battle| battle.battle.summary.clone())
            .filter(|summary| summary.role_of(*user_id).is_some() && summary.created_at >= since)
            .collect();
        battles.sort_by_key(|summary| std::cmp::Reverse((summary.created_at, summary.id.as_i32())));
        battles.truncate(limit as usize);
        Ok(battles)
    }

    async fn battle(&self, battle_id: BattleId) -> Result<Option<BuddyBattle>, BuddyError> {
        Ok(self
            .store()
            .battles
            .iter()
            .find(|battle| battle.battle.summary.id == battle_id)
            .map(|battle| battle.battle.clone()))
    }

    async fn mark_battle_seen(
        &self,
        battle_id: BattleId,
        defender_id: &UserId,
    ) -> Result<(), BuddyError> {
        let mut store = self.store();
        let now = store.clock.now;
        if let Some(stored) = store.battles.iter_mut().find(|battle| {
            battle.battle.summary.id == battle_id
                && battle.battle.summary.defender.user_id == Some(*defender_id)
        }) {
            stored.battle.summary.defender_seen_at.get_or_insert(now);
        }
        Ok(())
    }

    async fn standings(&self, month: AiBillingMonth) -> Result<Vec<BuddyStanding>, BuddyError> {
        let store = self.store();
        let standings = store
            .competitors(|_| true)
            .into_iter()
            .filter_map(|competitor| {
                let started: Vec<_> = store
                    .battles
                    .iter()
                    .filter(|battle| {
                        battle.challenger_id() == competitor.user_id() && month.contains(battle.day)
                    })
                    .collect();
                if started.is_empty() {
                    return None;
                }
                let mut colleague = BattleTally::default();
                let mut trainer_wins = 0;
                for battle in started {
                    let result = battle.battle.summary.result_for(BattleSide::Challenger);
                    match (battle.battle.summary.kind, result) {
                        (BattleKind::Colleague, BattleResult::Win) => colleague.wins += 1,
                        (BattleKind::Colleague, BattleResult::Loss) => colleague.losses += 1,
                        (BattleKind::Colleague, BattleResult::Draw) => colleague.draws += 1,
                        (BattleKind::Trainer, BattleResult::Win) => trainer_wins += 1,
                        (BattleKind::Trainer, _) => {}
                    }
                }
                Some(BuddyStanding {
                    competitor,
                    colleague,
                    trainer_wins,
                })
            })
            .collect();
        Ok(standings)
    }
}

/// Logged hours that are the same for every read: a connected user's, none for a user
/// who is not connected, or unavailable.
struct FakeHours {
    hours: Result<Option<Vec<DailyHours>>, ()>,
    reads: Mutex<Vec<(Date, Date)>>,
}

impl FakeHours {
    fn new(hours: Vec<DailyHours>) -> Arc<Self> {
        Self::answering(Ok(Some(hours)))
    }

    fn not_connected() -> Arc<Self> {
        Self::answering(Ok(None))
    }

    fn unavailable() -> Arc<Self> {
        Self::answering(Err(()))
    }

    fn answering(hours: Result<Option<Vec<DailyHours>>, ()>) -> Arc<Self> {
        Arc::new(Self {
            hours,
            reads: Mutex::new(Vec::new()),
        })
    }

    fn reads(&self) -> Vec<(Date, Date)> {
        self.reads.lock().unwrap().clone()
    }
}

#[async_trait]
impl LoggedHours for FakeHours {
    async fn daily_worked_hours(
        &self,
        user: UserId,
        range: (Date, Date),
    ) -> Result<Option<Vec<DailyHours>>, BuddyError> {
        assert_eq!(user, USER);
        self.reads.lock().unwrap().push(range);
        self.hours
            .clone()
            .map_err(|()| BuddyError::HoursUnavailable("time tracking is down".to_string()))
    }
}

/// Passes every notice it is given on to the test.
struct RecordingNotifier {
    notices: mpsc::UnboundedSender<BattleNotice>,
}

impl RecordingNotifier {
    fn new() -> (Arc<Self>, mpsc::UnboundedReceiver<BattleNotice>) {
        let (notices, received) = mpsc::unbounded_channel();
        (Arc::new(Self { notices }), received)
    }
}

#[async_trait]
impl BuddyNotifier for RecordingNotifier {
    async fn battle_fought(&self, notice: &BattleNotice) {
        // A test that does not listen has dropped the receiver.
        let _ = self.notices.send(notice.clone());
    }
}

type TestService = BuddyServiceImpl<MemoryBuddies, FakeHours, RecordingNotifier>;

fn service(buddies: &Arc<MemoryBuddies>, hours: &Arc<FakeHours>) -> TestService {
    service_with(buddies, hours, RecordingNotifier::new().0)
}

fn service_with(
    buddies: &Arc<MemoryBuddies>,
    hours: &Arc<FakeHours>,
    notifier: Arc<RecordingNotifier>,
) -> TestService {
    BuddyServiceImpl::new(
        buddies.clone(),
        hours.clone(),
        notifier,
        AiUsageTimeZone::parse("Europe/Stockholm").unwrap(),
    )
}

fn enrol_with(species_id: SpeciesId) -> BuddyProfileUpdate {
    BuddyProfileUpdate {
        enabled: Some(true),
        lead_species_id: Some(species_id),
        ..BuddyProfileUpdate::default()
    }
}

fn partner(species_id: SpeciesId) -> BuddyProfileUpdate {
    BuddyProfileUpdate {
        partner_species_id: Some(species_id),
        ..BuddyProfileUpdate::default()
    }
}

#[tokio::test]
async fn enrolment_needs_an_answer_and_a_lead_and_starts_counting_today() {
    let buddies = MemoryBuddies::new();
    let hours = FakeHours::new(Vec::new());
    let service = service(&buddies, &hours);

    let invalid: fn(&BuddyError) -> bool = |error| matches!(error, BuddyError::Invalid(_));
    let locked: fn(&BuddyError) -> bool = |error| matches!(error, BuddyError::PartnerLocked);
    let no_partner: fn(&BuddyError) -> bool = |error| matches!(error, BuddyError::NoPartner);
    for (update, expected) in [
        (BuddyProfileUpdate::default(), invalid),
        (
            BuddyProfileUpdate {
                enabled: Some(true),
                ..BuddyProfileUpdate::default()
            },
            invalid,
        ),
        (
            BuddyProfileUpdate {
                lead_species_id: Some(PIKACHU),
                ..BuddyProfileUpdate::default()
            },
            invalid,
        ),
        (
            BuddyProfileUpdate {
                partner_species_id: Some(BULBASAUR),
                ..enrol_with(PIKACHU)
            },
            locked,
        ),
        (
            BuddyProfileUpdate {
                partner_visible: Some(false),
                ..enrol_with(PIKACHU)
            },
            no_partner,
        ),
    ] {
        let error = service.update_profile(&USER, update).await.unwrap_err();
        assert!(expected(&error), "{update:?}: {error:?}");
        assert_eq!(buddies.profile_now(), None, "{update:?}");
    }

    // Declining the buddy still enrols, so the question is answered.
    let overview = service
        .update_profile(
            &USER,
            BuddyProfileUpdate {
                enabled: Some(false),
                ..enrol_with(PIKACHU)
            },
        )
        .await
        .unwrap();

    assert_eq!(
        overview.profile,
        BuddyProfile {
            user_id: USER,
            enabled: false,
            battle_notifications: true,
            xp_since: date!(2026 - 10 - 16),
            hours_synced_at: None,
            hours_sync_attempted_at: None,
            lead: Buddy {
                slot: BuddySlot::Lead,
                species_id: PIKACHU,
                visible: true,
                xp_base: 0,
            },
            partner: None,
        }
    );
    assert_eq!(overview.total_xp, 0);
    assert_eq!(overview.today.date, date!(2026 - 10 - 16));
    assert!(hours.reads().is_empty());
}

#[tokio::test]
async fn only_buddy_species_can_be_chosen() {
    let buddies = MemoryBuddies::new();
    let service = service(&buddies, &FakeHours::new(Vec::new()));

    // 514 is a national dex number without battle data or sprites.
    for species_id in [0, -1, 514, 650, i16::MAX].map(SpeciesId::new) {
        let error = service
            .update_profile(&USER, enrol_with(species_id))
            .await
            .unwrap_err();
        assert!(
            matches!(error, BuddyError::UnknownSpecies(id) if id == species_id),
            "{error:?}"
        );
    }
    assert_eq!(buddies.profile_now(), None);

    service
        .update_profile(&USER, enrol_with(PIKACHU))
        .await
        .unwrap();
    let error = service
        .update_profile(
            &USER,
            BuddyProfileUpdate {
                enabled: Some(false),
                lead_species_id: Some(CHARMANDER),
                partner_species_id: Some(SpeciesId::new(514)),
                ..BuddyProfileUpdate::default()
            },
        )
        .await
        .unwrap_err();

    assert!(matches!(error, BuddyError::UnknownSpecies(_)), "{error:?}");
    let profile = buddies.profile_now().unwrap();
    assert!(profile.enabled);
    assert_eq!(profile.lead.species_id, PIKACHU);
}

#[tokio::test]
async fn the_partner_unlocks_at_lead_level_10_and_earns_from_then_on() {
    let buddies = MemoryBuddies::new();
    let service = service(&buddies, &FakeHours::new(Vec::new()));
    service
        .update_profile(&USER, enrol_with(PIKACHU))
        .await
        .unwrap();

    // 509 XP is Lv 9; nothing in a rejected update applies.
    buddies.set_hours_xp(date!(2026 - 10 - 16), 509);
    let error = service
        .update_profile(
            &USER,
            BuddyProfileUpdate {
                battle_notifications: Some(false),
                ..partner(BULBASAUR)
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, BuddyError::PartnerLocked), "{error:?}");
    assert!(buddies.profile_now().unwrap().battle_notifications);

    // One battle later the lead is Lv 10.
    service.battle_trainer(&USER).await.unwrap();
    let overview = service
        .update_profile(&USER, partner(BULBASAUR))
        .await
        .unwrap();
    let new_partner = overview.profile.partner.unwrap();
    assert_eq!(overview.total_xp, 529);
    assert_eq!(new_partner.xp_base, 529);
    assert_eq!(overview.progress(&overview.profile.lead).level, 10);
    assert_eq!(overview.progress(&new_partner).xp, 0);

    // Changing species keeps the partner's XP; it can be hidden but stays.
    buddies.set_hours_xp(date!(2026 - 10 - 15), 300);
    let overview = service
        .update_profile(
            &USER,
            BuddyProfileUpdate {
                partner_visible: Some(false),
                ..partner(CHARMANDER)
            },
        )
        .await
        .unwrap();
    let changed = overview.profile.partner.unwrap();
    assert_eq!(
        changed,
        Buddy {
            slot: BuddySlot::Partner,
            species_id: CHARMANDER,
            visible: false,
            xp_base: 529,
        }
    );
    assert_eq!(overview.progress(&changed).xp, 300);
    assert_eq!(overview.progress(&overview.profile.lead).xp, 829);
}

#[tokio::test]
async fn showing_or_hiding_the_partner_needs_one() {
    let buddies = MemoryBuddies::new();
    let service = service(&buddies, &FakeHours::new(Vec::new()));
    service
        .update_profile(&USER, enrol_with(PIKACHU))
        .await
        .unwrap();
    buddies.set_hours_xp(date!(2026 - 10 - 16), 600);

    let error = service
        .update_profile(
            &USER,
            BuddyProfileUpdate {
                partner_visible: Some(false),
                ..BuddyProfileUpdate::default()
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(error, BuddyError::NoPartner), "{error:?}");

    // Creating and hiding the partner in one update is fine.
    let overview = service
        .update_profile(
            &USER,
            BuddyProfileUpdate {
                partner_visible: Some(false),
                ..partner(BULBASAUR)
            },
        )
        .await
        .unwrap();
    assert!(!overview.profile.partner.unwrap().visible);
}

#[tokio::test]
async fn opening_the_profile_stores_every_recent_day_once_a_minute() {
    let buddies = MemoryBuddies::new();
    let hours = FakeHours::new(vec![
        DailyHours {
            date: date!(2026 - 10 - 16),
            worked_hours: 6.5,
        },
        DailyHours {
            date: date!(2026 - 10 - 14),
            worked_hours: 11.0,
        },
    ]);
    let service = service(&buddies, &hours);
    assert!(service.get_profile(&USER).await.unwrap().is_none());
    assert!(hours.reads().is_empty());

    service
        .update_profile(&USER, enrol_with(PIKACHU))
        .await
        .unwrap();
    // A stale day from before the window, which a sync never touches, as the hours
    // were last read yesterday.
    buddies.set_hours_xp(date!(2026 - 09 - 01), 120);
    {
        let mut store = buddies.store();
        let profile = store.profiles.get_mut(&USER).unwrap();
        profile.xp_since = date!(2026 - 08 - 01);
        profile.hours_synced_at = Some(datetime!(2026-10-15 16:00 UTC));
    }

    let overview = service.get_profile(&USER).await.unwrap().unwrap();

    assert_eq!(
        hours.reads(),
        vec![(date!(2026 - 09 - 15), date!(2026 - 10 - 16))]
    );
    let days = buddies.store().days[&USER].clone();
    assert_eq!(days.len(), 33, "32 days in the window and the stale one");
    assert_eq!(days[&date!(2026 - 10 - 15)].xp, 0);
    assert_eq!(days[&date!(2026 - 10 - 14)].xp, 600);
    assert_eq!(days[&date!(2026 - 09 - 01)].xp, 120);
    assert_eq!(overview.total_xp, 120 + 600 + 390);
    assert_eq!(overview.today.logged_hours, 6.5);
    assert_eq!(overview.today.hours_xp, 390);
    assert_eq!(
        overview.profile.hours_synced_at,
        Some(datetime!(2026-10-16 10:00 UTC))
    );

    buddies.advance(Duration::seconds(59));
    service.get_profile(&USER).await.unwrap().unwrap();
    assert_eq!(hours.reads().len(), 1);

    buddies.advance(Duration::seconds(1));
    let overview = service.get_profile(&USER).await.unwrap().unwrap();
    assert_eq!(hours.reads().len(), 2);
    assert_eq!(
        overview.profile.hours_synced_at,
        Some(datetime!(2026-10-16 10:01 UTC))
    );
}

#[tokio::test]
async fn hours_count_from_the_enrolment_day() {
    let buddies = MemoryBuddies::new();
    let hours = FakeHours::new(vec![DailyHours {
        date: date!(2026 - 10 - 15),
        worked_hours: 8.0,
    }]);
    let service = service(&buddies, &hours);
    service
        .update_profile(&USER, enrol_with(PIKACHU))
        .await
        .unwrap();

    let overview = service.get_profile(&USER).await.unwrap().unwrap();

    assert_eq!(
        hours.reads(),
        vec![(date!(2026 - 10 - 16), date!(2026 - 10 - 16))]
    );
    assert_eq!(overview.total_xp, 0);
}

#[tokio::test]
async fn the_profile_shows_stored_xp_when_hours_cannot_be_synced() {
    for (buddies, hours) in [
        (MemoryBuddies::new(), FakeHours::unavailable()),
        (
            MemoryBuddies::with_failing_saves(true),
            FakeHours::new(vec![DailyHours {
                date: date!(2026 - 10 - 16),
                worked_hours: 2.0,
            }]),
        ),
    ] {
        let service = service(&buddies, &hours);
        service
            .update_profile(&USER, enrol_with(PIKACHU))
            .await
            .unwrap();
        buddies.set_hours_xp(date!(2026 - 10 - 16), 60);

        let overview = service.get_profile(&USER).await.unwrap().unwrap();

        assert_eq!(overview.total_xp, 60);
        assert_eq!(overview.profile.hours_synced_at, None);
        // A failed sync holds off the next one for a minute too.
        buddies.advance(Duration::seconds(59));
        service.get_profile(&USER).await.unwrap().unwrap();
        assert_eq!(hours.reads().len(), 1);
        buddies.advance(Duration::seconds(1));
        service.get_profile(&USER).await.unwrap().unwrap();
        assert_eq!(hours.reads().len(), 2);
    }
}

#[tokio::test]
async fn a_user_who_is_not_connected_to_time_tracking_keeps_their_stored_hours() {
    let buddies = MemoryBuddies::new();
    let hours = FakeHours::not_connected();
    let service = service(&buddies, &hours);
    service
        .update_profile(&USER, enrol_with(PIKACHU))
        .await
        .unwrap();
    buddies.store().profiles.get_mut(&USER).unwrap().xp_since = date!(2026 - 10 - 01);
    buddies.set_hours_xp(date!(2026 - 10 - 15), 480);

    let overview = service.get_profile(&USER).await.unwrap().unwrap();

    assert_eq!(hours.reads().len(), 1);
    assert_eq!(overview.total_xp, 480);
    assert_eq!(buddies.store().days[&USER].len(), 1);
    // No hours were read, so the next read still reaches back to the last one.
    assert_eq!(overview.profile.hours_synced_at, None);
}

#[tokio::test]
async fn hours_are_read_only_while_the_buddy_is_switched_on() {
    let buddies = MemoryBuddies::new();
    let hours = FakeHours::new(vec![DailyHours {
        date: date!(2026 - 10 - 16),
        worked_hours: 2.0,
    }]);
    let service = service(&buddies, &hours);
    service
        .update_profile(
            &USER,
            BuddyProfileUpdate {
                enabled: Some(false),
                ..enrol_with(PIKACHU)
            },
        )
        .await
        .unwrap();

    let overview = service.get_profile(&USER).await.unwrap().unwrap();

    assert!(hours.reads().is_empty());
    assert_eq!(overview.total_xp, 0);
    assert_eq!(overview.profile.hours_synced_at, None);

    // Switching back on reads the hours first, so the answer already counts them.
    let overview = service
        .update_profile(
            &USER,
            BuddyProfileUpdate {
                enabled: Some(true),
                ..BuddyProfileUpdate::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(hours.reads().len(), 1);
    assert_eq!(overview.total_xp, 120);
    assert!(overview.profile.hours_synced_at.is_some());

    let overview = service.get_profile(&USER).await.unwrap().unwrap();
    assert_eq!(
        hours.reads().len(),
        1,
        "the next minute's read was already made"
    );
    assert_eq!(overview.total_xp, 120);
}

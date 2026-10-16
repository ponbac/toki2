use std::time::Duration as StdDuration;

use super::*;
use crate::domain::models::{BattleEvent, BattleMon, BattleTeam};

const SQUIRTLE: SpeciesId = SpeciesId::new(7);

/// The buddies and a notifier whose notices the test can wait for.
fn battle_service(
    buddies: &Arc<MemoryBuddies>,
) -> (TestService, mpsc::UnboundedReceiver<BattleNotice>) {
    let (notifier, notices) = RecordingNotifier::new();
    (
        service_with(buddies, &FakeHours::new(Vec::new()), notifier),
        notices,
    )
}

/// Enrols `user_id` with a switched-on `lead`.
async fn enrol(service: &TestService, user_id: UserId, lead: SpeciesId) {
    service
        .update_profile(&user_id, enrol_with(lead))
        .await
        .unwrap();
}

/// Gives `user_id` the 540 XP of Lv 10 and a partner, which earns from then on.
async fn add_partner(
    service: &TestService,
    buddies: &MemoryBuddies,
    user_id: UserId,
    species_id: SpeciesId,
) {
    buddies.set_hours_xp_of(user_id, date!(2026 - 10 - 01), 540);
    service
        .update_profile(&user_id, partner(species_id))
        .await
        .unwrap();
}

fn species_of(team: &BattleTeam) -> Vec<(SpeciesId, u8)> {
    team.members()
        .iter()
        .map(|mon: &BattleMon| (mon.species_id, mon.level))
        .collect()
}

async fn next_notice(notices: &mut mpsc::UnboundedReceiver<BattleNotice>) -> BattleNotice {
    tokio::time::timeout(StdDuration::from_secs(5), notices.recv())
        .await
        .expect("a notice within 5 s")
        .expect("the notifier is alive")
}

#[tokio::test]
async fn a_colleague_battle_is_resolved_from_both_teams_and_earns_the_challenger_xp() {
    let buddies = MemoryBuddies::new();
    let (service, _) = battle_service(&buddies);
    enrol(&service, USER, PIKACHU).await;
    enrol(&service, ANNA, SQUIRTLE).await;
    // 600 XP is Lv 10.
    buddies.set_hours_xp(date!(2026 - 10 - 15), 600);

    let view = service.battle_colleague(&USER, &ANNA).await.unwrap();

    assert_eq!(view.role, BattleSide::Challenger);
    let summary = &view.battle.summary;
    assert_eq!(summary.kind, BattleKind::Colleague);
    assert_eq!(summary.created_at, datetime!(2026-10-16 10:00 UTC));
    assert_eq!(summary.challenger.user_id, Some(USER));
    assert_eq!(summary.challenger.name, "Ada Lovelace");
    assert_eq!(summary.defender.user_id, Some(ANNA));
    assert_eq!(summary.defender.name, "Anna Lind");
    assert_eq!(species_of(&summary.challenger.team), vec![(PIKACHU, 10)]);
    assert_eq!(species_of(&summary.defender.team), vec![(SQUIRTLE, 1)]);
    assert_eq!(view.battle.engine_version, battle::ENGINE_VERSION);
    // The stored replay is the engine's battle for the stored seed.
    let replay = battle::resolve(
        &summary.challenger.team,
        &summary.defender.team,
        view.battle.seed,
    );
    assert_eq!(view.battle.events, replay.events);
    assert_eq!(summary.winner, replay.winner);
    assert!(matches!(
        view.battle.events.last(),
        Some(BattleEvent::End { .. })
    ));

    let challenger = service.get_profile(&USER).await.unwrap().unwrap();
    assert_eq!(challenger.total_xp, 620);
    assert_eq!(challenger.today.battles, 1);
    assert_eq!(challenger.today.battle_xp(), 20);
    // Defending earns nothing.
    assert_eq!(buddies.store().tally(ANNA).total_xp(), 0);
}

#[tokio::test]
async fn colleague_battles_wait_an_hour_and_each_colleague_once_a_day() {
    let buddies = MemoryBuddies::new();
    let (service, _) = battle_service(&buddies);
    for user_id in [USER, ANNA, BERTIL] {
        enrol(&service, user_id, PIKACHU).await;
    }

    service.battle_colleague(&USER, &ANNA).await.unwrap();
    let error = service.battle_colleague(&USER, &BERTIL).await.unwrap_err();
    assert!(
        matches!(error, BuddyError::Cooldown { wait } if wait == Duration::hours(1)),
        "{error:?}"
    );
    assert_eq!(
        error.to_string(),
        "you can start your next colleague battle in 60 minutes"
    );

    // The wait is told in whole minutes, rounded up.
    buddies.advance(Duration::seconds(59 * 60 + 30));
    let opponents = service.opponents(&USER).await.unwrap();
    assert_eq!(
        opponents.next_battle_at,
        Some(datetime!(2026-10-16 11:00 UTC))
    );
    let error = service.battle_colleague(&USER, &BERTIL).await.unwrap_err();
    assert!(matches!(error, BuddyError::Cooldown { .. }), "{error:?}");
    assert_eq!(
        error.to_string(),
        "you can start your next colleague battle in 1 minute"
    );

    // An hour later the cooldown is over, but Anna was battled today already.
    buddies.advance(Duration::seconds(30));
    assert_eq!(service.opponents(&USER).await.unwrap().next_battle_at, None);
    let error = service.battle_colleague(&USER, &ANNA).await.unwrap_err();
    assert!(matches!(error, BuddyError::BattledToday), "{error:?}");
    service.battle_colleague(&USER, &BERTIL).await.unwrap();

    // Being battled does not hold anyone back.
    service.battle_colleague(&ANNA, &USER).await.unwrap();
    let opponents = service.opponents(&USER).await.unwrap();
    assert_eq!(
        opponents.next_battle_at,
        Some(datetime!(2026-10-16 12:00 UTC))
    );
    assert_eq!(
        opponents
            .opponents
            .iter()
            .map(|opponent| (opponent.user_id, opponent.battled_today))
            .collect::<Vec<_>>(),
        vec![(ANNA, true), (BERTIL, true)]
    );
    // The record counts the battles either side started.
    let anna = &opponents.opponents[0];
    assert_eq!(anna.record.wins + anna.record.losses + anna.record.draws, 2);

    // The next local day, Anna can be battled again.
    buddies.advance(Duration::hours(14));
    service.battle_colleague(&USER, &ANNA).await.unwrap();
    assert_eq!(buddies.battle_count(), 4);
}

#[tokio::test]
async fn the_daily_trainer_is_fought_once_a_day_apart_from_the_cooldown() {
    let buddies = MemoryBuddies::new();
    let (service, _) = battle_service(&buddies);
    enrol(&service, USER, PIKACHU).await;
    enrol(&service, ANNA, PIKACHU).await;
    let today = date!(2026 - 10 - 16);
    let trainer = battle::daily_trainer(today);

    let status = service.trainer(&USER).await.unwrap();
    assert_eq!(status.date, today);
    assert_eq!(status.name, trainer.name);
    // Without a partner, the trainer's lead fights alone.
    assert_eq!(status.team, trainer.team[..1]);
    assert_eq!(status.battle, None);

    // The trainer neither waits for nor starts the colleague cooldown.
    let fought = service.battle_trainer(&USER).await.unwrap();
    assert_eq!(fought.role, BattleSide::Challenger);
    assert_eq!(fought.battle.summary.kind, BattleKind::Trainer);
    assert_eq!(fought.battle.summary.defender.user_id, None);
    assert_eq!(fought.battle.summary.defender.name, trainer.name);
    assert_eq!(
        species_of(&fought.battle.summary.defender.team),
        trainer.team[..1]
    );
    assert_eq!(service.opponents(&USER).await.unwrap().next_battle_at, None);
    service.battle_colleague(&USER, &ANNA).await.unwrap();

    let error = service.battle_trainer(&USER).await.unwrap_err();
    assert!(
        matches!(error, BuddyError::TrainerAlreadyFought),
        "{error:?}"
    );
    let status = service.trainer(&USER).await.unwrap();
    assert_eq!(
        status.battle,
        Some(TrainerBattle {
            id: fought.battle.summary.id,
            winner: fought.battle.summary.winner,
        })
    );
    assert_eq!(
        status.battle.unwrap().result(),
        fought.battle.summary.result_for(BattleSide::Challenger)
    );
    // Others fight the same trainer on their own.
    service.battle_trainer(&ANNA).await.unwrap();

    // The next day brings a new trainer to fight.
    buddies.advance(Duration::days(1));
    let tomorrow = service.trainer(&USER).await.unwrap();
    assert_eq!(tomorrow.date, date!(2026 - 10 - 17));
    assert_eq!(tomorrow.battle, None);
    service.battle_trainer(&USER).await.unwrap();
    // Both trainer battles and the colleague battle earned XP.
    let overview = service.get_profile(&USER).await.unwrap().unwrap();
    assert_eq!(overview.total_xp, 60);
    assert_eq!(overview.today.battles, 1);
}

#[tokio::test]
async fn partners_fight_only_when_both_sides_have_one() {
    let buddies = MemoryBuddies::new();
    let (service, _) = battle_service(&buddies);
    for user_id in [USER, ANNA, BERTIL] {
        enrol(&service, user_id, PIKACHU).await;
    }
    add_partner(&service, &buddies, USER, BULBASAUR).await;
    add_partner(&service, &buddies, BERTIL, CHARMANDER).await;
    // A hidden partner still fights.
    service
        .update_profile(
            &USER,
            BuddyProfileUpdate {
                partner_visible: Some(false),
                ..BuddyProfileUpdate::default()
            },
        )
        .await
        .unwrap();
    // 160 more XP: the leads are at 700 XP, Lv 11, and the partners at 160, Lv 7.
    buddies.set_hours_xp_of(USER, date!(2026 - 10 - 02), 160);
    buddies.set_hours_xp_of(BERTIL, date!(2026 - 10 - 02), 160);

    let opponents = service.opponents(&USER).await.unwrap().opponents;
    assert_eq!(
        opponents
            .iter()
            .map(|opponent| (opponent.user_id, opponent.team.clone()))
            .collect::<Vec<_>>(),
        vec![
            (ANNA, vec![(PIKACHU, 1)]),
            (BERTIL, vec![(PIKACHU, 11), (CHARMANDER, 7)]),
        ]
    );
    assert_eq!(
        service.trainer(&USER).await.unwrap().team,
        battle::daily_trainer(date!(2026 - 10 - 16)).team
    );

    let pairs = service.battle_colleague(&USER, &BERTIL).await.unwrap();
    assert_eq!(
        species_of(&pairs.battle.summary.challenger.team),
        vec![(PIKACHU, 11), (BULBASAUR, 7)]
    );
    assert_eq!(
        species_of(&pairs.battle.summary.defender.team),
        vec![(PIKACHU, 11), (CHARMANDER, 7)]
    );

    buddies.advance(Duration::hours(1));
    let leads = service.battle_colleague(&USER, &ANNA).await.unwrap();
    assert_eq!(
        species_of(&leads.battle.summary.challenger.team),
        vec![(PIKACHU, 11)]
    );
    assert_eq!(
        species_of(&leads.battle.summary.defender.team),
        vec![(PIKACHU, 1)]
    );

    let trainer = service.battle_trainer(&USER).await.unwrap();
    assert_eq!(trainer.battle.summary.challenger.team.members().len(), 2);
    assert_eq!(trainer.battle.summary.defender.team.members().len(), 2);
}

#[tokio::test]
async fn only_switched_on_buddies_battle_and_are_battled() {
    let buddies = MemoryBuddies::new();
    let (service, _) = battle_service(&buddies);

    for error in [
        service.opponents(&USER).await.unwrap_err(),
        service.trainer(&USER).await.unwrap_err(),
        service.battle_trainer(&USER).await.unwrap_err(),
        service.battle_colleague(&USER, &ANNA).await.unwrap_err(),
    ] {
        assert!(matches!(error, BuddyError::NotEnrolled), "{error:?}");
    }

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
    for error in [
        service.opponents(&USER).await.unwrap_err(),
        service.trainer(&USER).await.unwrap_err(),
        service.battle_trainer(&USER).await.unwrap_err(),
        service.battle_colleague(&USER, &ANNA).await.unwrap_err(),
    ] {
        assert!(matches!(error, BuddyError::Disabled), "{error:?}");
    }

    service
        .update_profile(
            &USER,
            BuddyProfileUpdate {
                enabled: Some(true),
                ..BuddyProfileUpdate::default()
            },
        )
        .await
        .unwrap();
    service
        .update_profile(
            &BERTIL,
            BuddyProfileUpdate {
                enabled: Some(false),
                ..enrol_with(PIKACHU)
            },
        )
        .await
        .unwrap();
    // Anna never enrolled, Bertil switched his buddy off and 404 is nobody.
    for opponent_id in [ANNA, BERTIL, UserId::new(404)] {
        let error = service
            .battle_colleague(&USER, &opponent_id)
            .await
            .unwrap_err();
        assert!(
            matches!(error, BuddyError::OpponentUnavailable),
            "{opponent_id}: {error:?}"
        );
    }
    let error = service.battle_colleague(&USER, &USER).await.unwrap_err();
    assert!(matches!(error, BuddyError::Invalid(_)), "{error:?}");

    assert!(service.opponents(&USER).await.unwrap().opponents.is_empty());
    // A refused battle starts no cooldown and earns nothing.
    assert_eq!(buddies.battle_count(), 0);
    assert_eq!(service.opponents(&USER).await.unwrap().next_battle_at, None);
}

#[tokio::test]
async fn the_defender_is_told_about_a_battle_only_when_they_want_to_be() {
    let buddies = MemoryBuddies::new();
    let (service, mut notices) = battle_service(&buddies);
    for user_id in [USER, ANNA, BERTIL] {
        enrol(&service, user_id, PIKACHU).await;
    }
    add_partner(&service, &buddies, USER, BULBASAUR).await;
    add_partner(&service, &buddies, ANNA, SQUIRTLE).await;
    service
        .update_profile(
            &BERTIL,
            BuddyProfileUpdate {
                battle_notifications: Some(false),
                ..BuddyProfileUpdate::default()
            },
        )
        .await
        .unwrap();

    // Neither Bertil, who opted out, nor the trainer hears about a battle.
    service.battle_colleague(&USER, &BERTIL).await.unwrap();
    service.battle_trainer(&USER).await.unwrap();
    buddies.advance(Duration::hours(1));
    let battled = service.battle_colleague(&USER, &ANNA).await.unwrap();

    let notice = next_notice(&mut notices).await;
    assert_eq!(
        notice,
        BattleNotice {
            battle_id: battled.battle.summary.id,
            defender_id: ANNA,
            challenger_name: "Ada Lovelace".to_string(),
            defender_buddies: vec!["Pikachu".to_string(), "Squirtle".to_string()],
            result: battled.battle.summary.result_for(BattleSide::Defender),
        }
    );
    tokio::task::yield_now().await;
    assert!(notices.try_recv().is_err(), "only Anna was told");
}

#[tokio::test]
async fn each_participant_sees_a_battle_from_their_side_and_the_defender_sees_it_once() {
    let buddies = MemoryBuddies::new();
    let (service, _) = battle_service(&buddies);
    for user_id in [USER, ANNA, BERTIL] {
        enrol(&service, user_id, PIKACHU).await;
    }
    let battle_id = service
        .battle_colleague(&USER, &ANNA)
        .await
        .unwrap()
        .battle
        .summary
        .id;

    let mine = service.battles(&USER).await.unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].role, BattleSide::Challenger);
    assert!(mine[0].battle.seen_by(mine[0].role));
    let theirs = service.battles(&ANNA).await.unwrap();
    assert_eq!(theirs[0].role, BattleSide::Defender);
    assert!(!theirs[0].battle.seen_by(theirs[0].role));
    assert!(service.battles(&BERTIL).await.unwrap().is_empty());

    assert_eq!(
        service.battle(&ANNA, battle_id).await.unwrap().role,
        BattleSide::Defender
    );
    for (user_id, battle_id) in [(BERTIL, battle_id), (USER, BattleId::new(404))] {
        let error = service.battle(&user_id, battle_id).await.unwrap_err();
        assert!(matches!(error, BuddyError::BattleNotFound), "{error:?}");
    }
    let error = service
        .mark_battle_seen(&BERTIL, battle_id)
        .await
        .unwrap_err();
    assert!(matches!(error, BuddyError::NotParticipant), "{error:?}");
    let error = service
        .mark_battle_seen(&ANNA, BattleId::new(404))
        .await
        .unwrap_err();
    assert!(matches!(error, BuddyError::BattleNotFound), "{error:?}");

    // The challenger has nothing to mark; the defender's first look counts.
    service.mark_battle_seen(&USER, battle_id).await.unwrap();
    assert_eq!(
        service
            .battle(&ANNA, battle_id)
            .await
            .unwrap()
            .battle
            .summary
            .defender_seen_at,
        None
    );
    buddies.advance(Duration::minutes(5));
    service.mark_battle_seen(&ANNA, battle_id).await.unwrap();
    buddies.advance(Duration::minutes(5));
    service.mark_battle_seen(&ANNA, battle_id).await.unwrap();
    let seen = service.battle(&ANNA, battle_id).await.unwrap();
    assert_eq!(
        seen.battle.summary.defender_seen_at,
        Some(datetime!(2026-10-16 10:05 UTC))
    );
    assert!(service.battles(&ANNA).await.unwrap()[0]
        .battle
        .seen_by(BattleSide::Defender));

    // The list keeps the last 30 days.
    buddies.advance(Duration::days(30));
    assert!(service.battles(&USER).await.unwrap().is_empty());
}

#[tokio::test]
async fn the_leaderboard_defaults_to_the_local_month_and_shows_every_buddy() {
    let buddies = MemoryBuddies::new();
    let (service, _) = battle_service(&buddies);
    enrol(&service, USER, PIKACHU).await;
    enrol(&service, ANNA, PIKACHU).await;
    add_partner(&service, &buddies, USER, BULBASAUR).await;
    service
        .update_profile(
            &USER,
            BuddyProfileUpdate {
                partner_visible: Some(false),
                ..BuddyProfileUpdate::default()
            },
        )
        .await
        .unwrap();
    let battle = service.battle_colleague(&USER, &ANNA).await.unwrap();

    let leaderboard = service.leaderboard(None).await.unwrap();

    assert_eq!(leaderboard.month, AiBillingMonth::parse("2026-10").unwrap());
    // Only those who started a battle, with all their buddies, hidden or not.
    assert_eq!(leaderboard.rows.len(), 1);
    let row = &leaderboard.rows[0];
    assert_eq!(row.user_id, USER);
    assert_eq!(row.full_name, "Ada Lovelace");
    // 560 XP is Lv 10 for the lead; the partner's 20 XP is Lv 4.
    assert_eq!(row.team, vec![(PIKACHU, 10), (BULBASAUR, 4)]);
    let result = battle.battle.summary.result_for(BattleSide::Challenger);
    assert_eq!(
        row.colleague,
        BattleTally {
            wins: i64::from(result == BattleResult::Win),
            losses: i64::from(result == BattleResult::Loss),
            draws: i64::from(result == BattleResult::Draw),
        }
    );
    assert_eq!(row.trainer_wins, 0);

    let september = AiBillingMonth::parse("2026-09").unwrap();
    let earlier = service.leaderboard(Some(september)).await.unwrap();
    assert_eq!(earlier.month, september);
    assert!(earlier.rows.is_empty());
}

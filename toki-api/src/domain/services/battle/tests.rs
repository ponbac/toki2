use std::{collections::BTreeMap, time::Instant};

use serde_json::json;
use time::macros::date;

use super::{
    dex::{self, all_species_ids, species, Dex, DEX_JSON},
    engine::{damage, MAX_TURNS},
    rng::Mulberry32,
    team::battle_level,
    trainer::{date_seed, TRAINER_LEVEL},
    *,
};
use crate::domain::models::{
    BattleError, BattleEvent, BattleMon, BattleMove, BattleOutcome, BattleSide, BattleStats,
    BattleTeam, MonType, MoveCategory, SpeciesId,
};

const PIKACHU: SpeciesId = SpeciesId::new(25);
const CHARIZARD: SpeciesId = SpeciesId::new(6);
const MEWTWO: SpeciesId = SpeciesId::new(150);
const MAGIKARP: SpeciesId = SpeciesId::new(129);
const BLISSEY: SpeciesId = SpeciesId::new(242);

fn team(members: &[(SpeciesId, u8)]) -> BattleTeam {
    build_team(members).expect("the test team is valid")
}

fn battle_move(name: &str, move_type: MonType, power: u16) -> BattleMove {
    BattleMove {
        name: name.to_owned(),
        move_type,
        category: MoveCategory::Physical,
        power,
        accuracy: Some(100),
        pp: 20,
        priority: 0,
        crit_stage: 0,
    }
}

/// A hand-made buddy, for rules that must not depend on the generated dex.
fn custom_mon(
    types: &[MonType],
    max_hp: u16,
    stats: BattleStats,
    moves: Vec<BattleMove>,
) -> BattleMon {
    BattleMon {
        species_id: SpeciesId::new(1),
        name: "Testmon".to_owned(),
        level: 50,
        types: types.to_vec(),
        max_hp,
        stats,
        moves,
    }
}

fn flat_stats(value: u16) -> BattleStats {
    BattleStats {
        attack: value,
        defense: value,
        special_attack: value,
        special_defense: value,
        speed: value,
    }
}

fn points(winner: Option<BattleSide>, side: BattleSide) -> f64 {
    match winner {
        Some(winner) if winner == side => 1.0,
        Some(_) => 0.0,
        None => 0.5,
    }
}

fn next_seed(rng: &mut Mulberry32) -> u32 {
    (rng.next_f64() * 4_294_967_296.0) as u32
}

/// Replays the events against the teams and checks every rule a replay relies on:
/// sends in order, every move followed by its result, HP that only goes down by the
/// damage dealt, faints exactly at 0 HP, and a final `end` that names the side
/// whose opponent ran out of buddies.
fn assert_consistent(challenger: &BattleTeam, defender: &BattleTeam, outcome: &BattleOutcome) {
    let teams = [challenger.members(), defender.members()];
    let slot = |side: BattleSide| match side {
        BattleSide::Challenger => 0,
        BattleSide::Defender => 1,
    };
    let mut active: [Option<usize>; 2] = [None, None];
    let mut hp = [0_u16; 2];
    let mut moves = 0;
    let events = &outcome.events;

    assert_eq!(
        events[..2],
        [
            BattleEvent::Send {
                side: BattleSide::Defender,
                index: 0
            },
            BattleEvent::Send {
                side: BattleSide::Challenger,
                index: 0
            },
        ]
    );
    for (position, event) in events.iter().enumerate() {
        let next = events.get(position + 1);
        match *event {
            BattleEvent::Send { side, index } => {
                let s = slot(side);
                let expected = active[s].map_or(0, |current| current + 1);
                assert_eq!(usize::from(index), expected, "sends go in team order");
                assert!(
                    active[s].is_none() || hp[s] == 0,
                    "only a fainted buddy is replaced"
                );
                active[s] = Some(expected);
                hp[s] = teams[s][expected].max_hp;
            }
            BattleEvent::Move { side, move_index } => {
                moves += 1;
                let s = slot(side);
                let mon = &teams[s][active[s].expect("the attacker was sent out")];
                assert!(hp[s] > 0, "a fainted buddy does not attack");
                assert!(usize::from(move_index) < mon.moves.len());
                let target = side.opponent();
                assert!(
                    matches!(
                        next,
                        Some(BattleEvent::Miss { side: attacker }) if *attacker == side
                    ) || matches!(
                        next,
                        Some(BattleEvent::Immune { side: t } | BattleEvent::Hit { side: t, .. }) if *t == target
                    ),
                    "a move is followed by its result, got {next:?}"
                );
            }
            BattleEvent::Hit {
                side,
                damage,
                hp_after,
                effectiveness,
                ..
            } => {
                let s = slot(side);
                assert!(
                    damage >= 1 && damage <= hp[s],
                    "damage {damage} with {} HP left",
                    hp[s]
                );
                assert_eq!(hp_after, hp[s] - damage);
                assert!(effectiveness > 0.0);
                hp[s] = hp_after;
                if hp_after == 0 {
                    assert_eq!(next, Some(&BattleEvent::Faint { side }));
                }
            }
            BattleEvent::Faint { side } => assert_eq!(hp[slot(side)], 0),
            BattleEvent::Miss { .. } | BattleEvent::Immune { .. } => {}
            BattleEvent::End { winner } => {
                assert_eq!(position, events.len() - 1, "end is the last event");
                assert_eq!(winner, outcome.winner);
                if let Some(winner) = winner {
                    let loser = slot(winner.opponent());
                    assert_eq!(hp[loser], 0, "the loser's last buddy fainted");
                    assert_eq!(active[loser], Some(teams[loser].len() - 1));
                    assert!(hp[slot(winner)] > 0);
                }
            }
        }
    }
    assert!(matches!(events.last(), Some(BattleEvent::End { .. })));
    assert!(moves <= 2 * MAX_TURNS);
}

#[test]
fn embedded_dex_is_valid_and_lists_exactly_the_buddies_with_sprites() {
    Dex::parse(DEX_JSON).expect("the embedded dex parses and passes its checks");

    let generated = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../app/src/lib/buddy/species.generated.ts"
    ))
    .expect("the sprite species list is readable");
    let pattern = regex::Regex::new(r#"\[(\d+), "([^"]+)"\]"#).expect("valid pattern");
    let sprites: BTreeMap<SpeciesId, String> = pattern
        .captures_iter(&generated)
        .map(|captures| {
            let id = captures[1].parse().expect("a dex number");
            (SpeciesId::new(id), captures[2].to_owned())
        })
        .collect();
    let dex: BTreeMap<SpeciesId, String> = all_species_ids()
        .map(|id| (id, species(id).expect("listed species exist").name.clone()))
        .collect();

    assert!(sprites.len() > 600, "parsed the sprite list");
    assert_eq!(
        dex, sprites,
        "rerun `bun run buddy:dex` after regenerating the sprites"
    );
    assert!(is_known_species(PIKACHU));
    // Simisear has no complete sprite atlas.
    assert!(!is_known_species(SpeciesId::new(514)));
    assert!(!is_known_species(SpeciesId::new(0)));
    assert!(!is_known_species(SpeciesId::new(650)));
}

#[test]
fn dex_parsing_rejects_malformed_data() {
    let mut file: serde_json::Value = serde_json::from_str(DEX_JSON).expect("valid JSON");
    let parse = |file: &serde_json::Value| Dex::parse(&file.to_string());
    assert!(parse(&file).is_ok());

    let mut fairy = file.clone();
    fairy["types"][0] = json!("Fairy");
    assert!(
        parse(&fairy).is_err(),
        "there is no Fairy type in generation 5"
    );

    let mut no_moves = file.clone();
    no_moves["species"]["25"]["moves"] = json!([]);
    assert!(parse(&no_moves).is_err());

    let mut bad_multiplier = file.clone();
    bad_multiplier["typeChart"]["Fire"]["Grass"] = json!(3);
    assert!(parse(&bad_multiplier).is_err());

    file["species"]["25"]["types"] = json!(["Electric", "Electric"]);
    assert!(parse(&file).is_err());
}

#[test]
fn mulberry32_matches_the_javascript_reference() {
    // `bun -e` with the JavaScript mulberry32, first five outputs times 2^32.
    let vectors: [(u32, [u32; 5]); 5] = [
        (0, [1144304738, 1416247, 958946056, 627933444, 2007157716]),
        (
            1,
            [2693262067, 11749833, 2265367787, 4213581821, 4159151403],
        ),
        (
            42,
            [2581720956, 1925393290, 3661312704, 2876485805, 750819978],
        ),
        (
            20261016,
            [802720415, 2262451170, 158727047, 817472241, 1171105599],
        ),
        (
            0xFFFF_FFFF,
            [3850105811, 813802916, 3073704848, 4054706436, 3630262831],
        ),
    ];
    for (seed, expected) in vectors {
        let mut rng = Mulberry32::new(seed);
        let actual = expected.map(|_| rng.next_f64());
        assert_eq!(
            actual,
            expected.map(|n| f64::from(n) / 4_294_967_296.0),
            "seed {seed}"
        );
    }
    let mut rng = Mulberry32::new(0);
    assert_eq!(rng.next_f64(), 0.266_429_208_684_712_65);
}

#[test]
fn stats_scale_base_stats_into_the_band_at_the_battle_level() {
    // Expected values from the formulas evaluated in JavaScript.
    let cases = [
        // BST 300 is raised to 380.
        (PIKACHU, 31, 117, [88, 57, 82, 69, 132]),
        // BST 680 is lowered to 540.
        (MEWTWO, 100, 170, [115, 98, 152, 98, 132]),
        // BST 534 is inside the band.
        (CHARIZARD, 50, 154, [105, 99, 130, 106, 121]),
        (MAGIKARP, 1, 107, [37, 117, 46, 55, 162]),
        (BLISSEY, 57, 335, [30, 30, 96, 157, 76]),
    ];
    for (id, level, max_hp, [attack, defense, special_attack, special_defense, speed]) in cases {
        let built = team(&[(id, level)]);
        let mon = &built.members()[0];
        assert_eq!(mon.max_hp, max_hp, "{} HP", mon.name);
        assert_eq!(
            mon.stats,
            BattleStats {
                attack,
                defense,
                special_attack,
                special_defense,
                speed
            },
            "{} stats",
            mon.name
        );
    }
    assert_eq!(battle_level(1), 47.0 + 1.0 / 15.0);
    assert_eq!(battle_level(30), 49.0);
    assert_eq!(battle_level(100), 47.0 + 100.0 / 15.0);
}

#[test]
fn team_snapshots_copy_the_species_and_serialise_in_the_stored_shape() {
    let built = team(&[(PIKACHU, 31)]);
    let pikachu = &built.members()[0];
    let dex_entry = species(PIKACHU).expect("Pikachu is a buddy");
    assert_eq!(pikachu.name, "Pikachu");
    assert_eq!(pikachu.level, 31);
    assert_eq!(pikachu.types, [MonType::Electric]);
    assert_eq!(pikachu.moves, dex_entry.moves);

    let value = serde_json::to_value(&built).expect("serialisable");
    assert_eq!(value[0]["speciesId"], 25);
    assert_eq!(value[0]["name"], "Pikachu");
    assert_eq!(value[0]["maxHp"], 117);
    assert_eq!(value[0]["types"], json!(["Electric"]));
    assert_eq!(value[0]["stats"]["specialAttack"], 82);
    assert_eq!(
        value[0]["moves"][0],
        json!({ "name": "Thunderbolt", "type": "Electric", "category": "special", "power": 95,
                "accuracy": 100, "pp": 15, "priority": 0, "critStage": 0 })
    );

    let round_trip: BattleTeam = serde_json::from_value(value).expect("deserialisable");
    assert_eq!(round_trip, built);
    assert!(serde_json::from_value::<BattleTeam>(json!([])).is_err());
}

#[test]
fn battle_events_serialise_as_the_replay_expects() {
    let cases = [
        (
            BattleEvent::Send {
                side: BattleSide::Defender,
                index: 0,
            },
            json!({ "type": "send", "side": "defender", "index": 0 }),
        ),
        (
            BattleEvent::Move {
                side: BattleSide::Challenger,
                move_index: 2,
            },
            json!({ "type": "move", "side": "challenger", "moveIndex": 2 }),
        ),
        (
            BattleEvent::Miss {
                side: BattleSide::Challenger,
            },
            json!({ "type": "miss", "side": "challenger" }),
        ),
        (
            BattleEvent::Immune {
                side: BattleSide::Defender,
            },
            json!({ "type": "immune", "side": "defender" }),
        ),
        (
            BattleEvent::Hit {
                side: BattleSide::Defender,
                damage: 31,
                hp_after: 86,
                effectiveness: 2.0,
                crit: false,
            },
            json!({ "type": "hit", "side": "defender", "damage": 31, "hpAfter": 86,
                    "effectiveness": 2.0, "crit": false }),
        ),
        (
            BattleEvent::Faint {
                side: BattleSide::Defender,
            },
            json!({ "type": "faint", "side": "defender" }),
        ),
        (
            BattleEvent::End {
                winner: Some(BattleSide::Challenger),
            },
            json!({ "type": "end", "winner": "challenger" }),
        ),
        (
            BattleEvent::End { winner: None },
            json!({ "type": "end", "winner": null }),
        ),
    ];
    for (event, expected) in cases {
        assert_eq!(
            serde_json::to_value(&event).expect("serialisable"),
            expected
        );
        assert_eq!(
            serde_json::from_value::<BattleEvent>(expected).expect("deserialisable"),
            event
        );
    }
}

#[test]
fn build_team_rejects_unknown_species_bad_levels_and_team_sizes() {
    assert_eq!(
        build_team(&[(SpeciesId::new(514), 10)]),
        Err(BattleError::UnknownSpecies(SpeciesId::new(514)))
    );
    assert_eq!(
        build_team(&[(PIKACHU, 0)]),
        Err(BattleError::InvalidLevel(0))
    );
    assert_eq!(
        build_team(&[(PIKACHU, 101)]),
        Err(BattleError::InvalidLevel(101))
    );
    assert_eq!(build_team(&[]), Err(BattleError::InvalidTeamSize(0)));
    assert_eq!(
        build_team(&[(PIKACHU, 5), (PIKACHU, 5), (PIKACHU, 5)]),
        Err(BattleError::InvalidTeamSize(3))
    );
    assert!(build_team(&[(PIKACHU, 1), (MEWTWO, 100)]).is_ok());

    let moveless = custom_mon(&[MonType::Normal], 100, flat_stats(50), vec![]);
    assert!(matches!(
        BattleTeam::new(vec![moveless]),
        Err(BattleError::InvalidMoves { .. })
    ));
    let zero_defense = custom_mon(
        &[MonType::Normal],
        100,
        BattleStats {
            defense: 0,
            ..flat_stats(50)
        },
        vec![battle_move("Tackle", MonType::Normal, 50)],
    );
    assert!(matches!(
        BattleTeam::new(vec![zero_defense]),
        Err(BattleError::InvalidMon(_))
    ));
}

#[test]
fn type_chart_is_generation_5() {
    use MonType::*;
    let cases = [
        (Ghost, vec![Normal], 0.0),
        (Normal, vec![Ghost], 0.0),
        (Ghost, vec![Steel], 0.5),
        (Dark, vec![Steel], 0.5),
        (Electric, vec![Ground], 0.0),
        (Electric, vec![Water, Flying], 4.0),
        (Fire, vec![Grass], 2.0),
        (Grass, vec![Fire, Flying], 0.25),
        (Water, vec![Water, Grass], 0.25),
        (Dragon, vec![Dragon], 2.0),
        (Psychic, vec![Dark], 0.0),
        (Normal, vec![Normal], 1.0),
    ];
    for (attacking, defending, expected) in cases {
        assert_eq!(
            dex::effectiveness(attacking, &defending),
            expected,
            "{attacking:?} on {defending:?}"
        );
    }

    let file: serde_json::Value = serde_json::from_str(DEX_JSON).expect("valid JSON");
    let types = file["types"].as_array().expect("a type list");
    assert!(!types.contains(&json!("Fairy")));
    assert_eq!(file["types"], json!(MonType::ALL));
    for (index, mon_type) in MonType::ALL.into_iter().enumerate() {
        assert_eq!(mon_type.index(), index);
    }
}

#[test]
fn the_same_seed_always_gives_the_same_battle() {
    let challenger = team(&[(PIKACHU, 31), (CHARIZARD, 12)]);
    let defender = team(&[(MEWTWO, 3), (MAGIKARP, 80)]);
    let first = resolve(&challenger, &defender, 0x5F3A_91C2);
    assert_eq!(resolve(&challenger, &defender, 0x5F3A_91C2), first);

    let differing = (0..20)
        .filter(|seed| resolve(&challenger, &defender, *seed).events != first.events)
        .count();
    assert!(differing >= 19, "other seeds give other battles");
}

/// Pins the rules. If this battle changes, a rule changed: bump `ENGINE_VERSION`, so
/// that battles stored under the old rules can be told apart, then pin the new battle.
#[test]
fn the_rules_of_this_engine_version_decide_a_known_battle() {
    use BattleEvent::{End, Faint};
    use BattleSide::{Challenger, Defender};
    let send = |side, index| BattleEvent::Send { side, index };
    let pick = |side, move_index| BattleEvent::Move { side, move_index };
    let hit = |side, damage, hp_after, crit| BattleEvent::Hit {
        side,
        damage,
        hp_after,
        effectiveness: 1.0,
        crit,
    };
    let challenger = team(&[(PIKACHU, 31), (CHARIZARD, 12)]);
    let defender = team(&[(MEWTWO, 3), (MAGIKARP, 80)]);

    let outcome = resolve(&challenger, &defender, 0x5F3A_91C2);

    assert_eq!(ENGINE_VERSION, 1, "pin the battle of a new version");
    assert_eq!(
        outcome.events,
        [
            send(Defender, 0),
            send(Challenger, 0),
            pick(Challenger, 1),
            hit(Defender, 30, 121, false),
            pick(Defender, 0),
            hit(Challenger, 117, 0, true),
            Faint { side: Challenger },
            send(Challenger, 1),
            pick(Defender, 0),
            hit(Challenger, 43, 104, false),
            pick(Challenger, 2),
            hit(Defender, 40, 81, false),
            pick(Defender, 3),
            hit(Challenger, 33, 71, false),
            pick(Challenger, 2),
            hit(Defender, 34, 47, false),
            pick(Defender, 3),
            hit(Challenger, 32, 39, false),
            pick(Challenger, 2),
            hit(Defender, 40, 7, false),
            pick(Defender, 0),
            hit(Challenger, 39, 0, false),
            Faint { side: Challenger },
            End {
                winner: Some(Defender)
            },
        ]
    );
    assert_eq!(outcome.winner, Some(Defender));
}

/// Damage worked out by hand at Lv 50, where the level factor is 22.
#[test]
fn damage_applies_each_step_in_order_in_whole_numbers() {
    let mon = |mon_type, stats| custom_mon(&[mon_type], 100, stats, Vec::new());
    let normal = mon(MonType::Normal, flat_stats(100));
    let fire = mon(MonType::Fire, flat_stats(100));
    let ember = battle_move("Ember", MonType::Fire, 80);

    // 22 × 80 × 100 / 100 / 50 + 2 = 37.
    assert_eq!(damage(&normal, &fire, &ember, false, 100, 1.0), 37);
    // A critical hit doubles to 74, the roll makes 65, the same type 97 and 4× 388.
    assert_eq!(damage(&fire, &normal, &ember, true, 88, 4.0), 388);
    // The roll makes 31, the same type 46 and ¼× 11.
    assert_eq!(damage(&fire, &normal, &ember, false, 85, 0.25), 11);
    // Power counts as 60 to 100: 22 × 60 × 100 / 100 / 50 + 2 = 28, and 46 for 100.
    let weak_move = battle_move("Water Gun", MonType::Water, 20);
    let strong_move = battle_move("Hydro Pump", MonType::Water, 150);
    assert_eq!(damage(&normal, &fire, &weak_move, false, 100, 1.0), 28);
    assert_eq!(damage(&normal, &fire, &strong_move, false, 100, 1.0), 46);
    // Special moves use the special stats: 22 × 80 × 150 / 50 / 50 + 2 = 107.
    let swift = BattleMove {
        category: MoveCategory::Special,
        ..battle_move("Swift", MonType::Normal, 80)
    };
    let caster = mon(
        MonType::Fire,
        BattleStats {
            special_attack: 150,
            ..flat_stats(100)
        },
    );
    let frail = mon(
        MonType::Fire,
        BattleStats {
            special_defense: 50,
            ..flat_stats(100)
        },
    );
    assert_eq!(damage(&caster, &frail, &swift, false, 100, 1.0), 107);
    // Every hit deals at least 1: 22 × 60 × 5 / 999 / 50 + 2 = 2, then 1, then 0.
    let feeble = mon(
        MonType::Normal,
        BattleStats {
            attack: 5,
            ..flat_stats(100)
        },
    );
    let wall = mon(
        MonType::Rock,
        BattleStats {
            defense: 999,
            ..flat_stats(100)
        },
    );
    assert_eq!(damage(&feeble, &wall, &weak_move, false, 85, 0.5), 1);
}

#[test]
fn one_on_one_and_two_on_two_battles_run_to_a_consistent_end() {
    let ids: Vec<SpeciesId> = all_species_ids().collect();
    let mut rng = Mulberry32::new(7);
    let mut winners = [0; 3];
    for round in 0..400 {
        let size = if round % 2 == 0 { 1 } else { 2 };
        let mut pick = || (ids[rng.index(ids.len())], 1 + rng.index(100) as u8);
        let challenger_members: Vec<_> = (0..size).map(|_| pick()).collect();
        let defender_members: Vec<_> = (0..size).map(|_| pick()).collect();
        let challenger = team(&challenger_members);
        let defender = team(&defender_members);

        let outcome = resolve(&challenger, &defender, next_seed(&mut rng));
        assert_consistent(&challenger, &defender, &outcome);
        winners[match outcome.winner {
            Some(BattleSide::Challenger) => 0,
            Some(BattleSide::Defender) => 1,
            None => 2,
        }] += 1;

        if size == 2 && outcome.winner.is_some() {
            let loser = outcome.winner.map(BattleSide::opponent);
            let loser_sends = outcome
                .events
                .iter()
                .filter(
                    |event| matches!(event, BattleEvent::Send { side, .. } if Some(*side) == loser),
                )
                .count();
            assert_eq!(loser_sends, 2, "the loser sent out both buddies");
        }
    }
    assert!(
        winners[0] > 100 && winners[1] > 100,
        "both sides win often: {winners:?}"
    );
}

#[test]
fn mutual_immunity_is_a_draw_after_the_turn_limit() {
    let normal = custom_mon(
        &[MonType::Normal],
        100,
        flat_stats(50),
        vec![battle_move("Tackle", MonType::Normal, 50)],
    );
    let ghost = custom_mon(
        &[MonType::Ghost],
        100,
        flat_stats(50),
        vec![battle_move("Lick", MonType::Ghost, 30)],
    );
    let challenger = BattleTeam::new(vec![normal]).expect("valid team");
    let defender = BattleTeam::new(vec![ghost]).expect("valid team");

    let outcome = resolve(&challenger, &defender, 1);
    assert_consistent(&challenger, &defender, &outcome);
    assert_eq!(outcome.winner, None);
    assert_eq!(
        outcome.events.last(),
        Some(&BattleEvent::End { winner: None })
    );
    let count = |predicate: fn(&BattleEvent) -> bool| {
        outcome.events.iter().filter(|e| predicate(e)).count()
    };
    assert_eq!(
        count(|event| matches!(event, BattleEvent::Move { .. })),
        2 * MAX_TURNS
    );
    assert_eq!(
        count(|event| matches!(event, BattleEvent::Immune { .. })),
        2 * MAX_TURNS
    );
}

#[test]
fn a_battle_nobody_can_finish_in_time_is_a_draw() {
    // Each hit deals 1 or 2 damage, far too little to faint 999 HP in 60 turns.
    let wall = || {
        custom_mon(
            &[MonType::Normal],
            999,
            BattleStats {
                attack: 5,
                defense: 999,
                special_defense: 999,
                ..flat_stats(50)
            },
            vec![battle_move("Tackle", MonType::Fire, 60)],
        )
    };
    let challenger = BattleTeam::new(vec![wall(), wall()]).expect("valid team");
    let defender = BattleTeam::new(vec![wall()]).expect("valid team");

    let outcome = resolve(&challenger, &defender, 3);
    assert_consistent(&challenger, &defender, &outcome);
    assert_eq!(outcome.winner, None);
    let hits = outcome
        .events
        .iter()
        .filter(|event| matches!(event, BattleEvent::Hit { .. }))
        .count();
    assert!(hits > 0 && hits <= 2 * MAX_TURNS);
}

#[test]
fn priority_beats_speed_and_speed_beats_the_coin_flip() {
    let fast = custom_mon(
        &[MonType::Normal],
        500,
        BattleStats {
            speed: 200,
            ..flat_stats(50)
        },
        vec![battle_move("Tackle", MonType::Normal, 60)],
    );
    let slow_with_priority = custom_mon(
        &[MonType::Normal],
        500,
        BattleStats {
            speed: 10,
            ..flat_stats(50)
        },
        vec![BattleMove {
            priority: 1,
            ..battle_move("Quick Attack", MonType::Normal, 60)
        }],
    );
    let slow = custom_mon(
        &[MonType::Normal],
        500,
        BattleStats {
            speed: 10,
            ..flat_stats(50)
        },
        vec![battle_move("Tackle", MonType::Normal, 60)],
    );
    let first_mover = |challenger: &BattleMon, defender: &BattleMon, seed: u32| {
        let outcome = resolve(
            &BattleTeam::new(vec![challenger.clone()]).expect("valid team"),
            &BattleTeam::new(vec![defender.clone()]).expect("valid team"),
            seed,
        );
        outcome
            .events
            .iter()
            .find_map(|event| match event {
                BattleEvent::Move { side, .. } => Some(*side),
                _ => None,
            })
            .expect("someone moves")
    };
    for seed in 0..10 {
        assert_eq!(first_mover(&slow, &fast, seed), BattleSide::Defender);
        assert_eq!(
            first_mover(&slow_with_priority, &fast, seed),
            BattleSide::Challenger
        );
    }
}

#[test]
fn the_daily_trainer_is_seeded_by_the_date() {
    assert_eq!(date_seed(date!(2026 - 10 - 16)), 20261016);
    assert_eq!(date_seed(date!(2027 - 01 - 05)), 20270105);

    let today = daily_trainer(date!(2026 - 10 - 16));
    assert_eq!(daily_trainer(date!(2026 - 10 - 16)), today);
    assert_eq!(today.team.len(), 2);
    assert_ne!(today.team[0].0, today.team[1].0, "two distinct species");
    assert!(today
        .team
        .iter()
        .all(|(id, level)| is_known_species(*id) && *level == TRAINER_LEVEL));
    assert!(build_team(&today.team).is_ok());

    let month: Vec<_> = (1..=28)
        .map(|day| {
            daily_trainer(
                time::Date::from_calendar_date(2026, time::Month::February, day).expect("a date"),
            )
        })
        .collect();
    let distinct_teams = month
        .iter()
        .map(|trainer| trainer.team.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let distinct_names = month
        .iter()
        .map(|trainer| trainer.name.clone())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(distinct_teams.len() >= 27, "the team changes every day");
    assert!(distinct_names.len() >= 8, "the names vary");
}

/// Mean of the values at sorted positions `[from, to)` of `n`, as fractions.
fn band_mean(sorted: &[f64], from: f64, to: f64) -> f64 {
    let n = sorted.len() as f64;
    let slice = &sorted[(from * n) as usize..(to * n) as usize];
    slice.iter().sum::<f64>() / slice.len() as f64
}

/// Ranks with ties sharing their average rank.
fn ranks(values: &[f64]) -> Vec<f64> {
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|a, b| values[*a].total_cmp(&values[*b]));
    let mut ranks = vec![0.0; values.len()];
    let mut start = 0;
    while start < order.len() {
        let mut end = start + 1;
        while end < order.len() && values[order[end]] == values[order[start]] {
            end += 1;
        }
        let average = (start + end - 1) as f64 / 2.0;
        for &index in &order[start..end] {
            ranks[index] = average;
        }
        start = end;
    }
    ranks
}

fn spearman(x: &[f64], y: &[f64]) -> f64 {
    let (rx, ry) = (ranks(x), ranks(y));
    let n = x.len() as f64;
    let (mx, my) = (rx.iter().sum::<f64>() / n, ry.iter().sum::<f64>() / n);
    let covariance: f64 = rx.iter().zip(&ry).map(|(a, b)| (a - mx) * (b - my)).sum();
    let spread_x: f64 = rx.iter().map(|a| (a - mx).powi(2)).sum();
    let spread_y: f64 = ry.iter().map(|b| (b - my).powi(2)).sum();
    covariance / (spread_x * spread_y).sqrt()
}

/// The balance targets of issue #180: at equal levels, the weakest species by base
/// stat total still win a fair share and the strongest do not always win; and a
/// long-serving buddy beats a new one more often than not, but far from always.
#[test]
fn balance_keeps_weak_species_viable_and_levels_meaningful() {
    const OPPONENTS_PER_SPECIES: usize = 400;
    const LEVEL_GAP_SAMPLES: usize = 2500;
    let started = Instant::now();
    let ids: Vec<SpeciesId> = all_species_ids().collect();
    let mut rng = Mulberry32::new(180);

    // Every species against random opponents, 1v1 at Lv 30 each.
    let teams: Vec<BattleTeam> = ids.iter().map(|id| team(&[(*id, 30)])).collect();
    let win_rates: Vec<f64> = teams
        .iter()
        .map(|own| {
            let total: f64 = (0..OPPONENTS_PER_SPECIES)
                .map(|_| {
                    let opponent = &teams[rng.index(teams.len())];
                    let seed = next_seed(&mut rng);
                    points(resolve(own, opponent, seed).winner, BattleSide::Challenger)
                })
                .sum();
            total / OPPONENTS_PER_SPECIES as f64
        })
        .collect();
    let totals: Vec<f64> = ids
        .iter()
        .map(|id| {
            species(*id)
                .expect("listed species exist")
                .base_stats
                .iter()
                .map(|s| f64::from(*s))
                .sum()
        })
        .collect();

    let mut by_total: Vec<usize> = (0..ids.len()).collect();
    by_total.sort_by(|a, b| {
        totals[*a]
            .total_cmp(&totals[*b])
            .then(ids[*a].cmp(&ids[*b]))
    });
    let rates_by_total: Vec<f64> = by_total.iter().map(|index| win_rates[*index]).collect();
    let bottom_decile = band_mean(&rates_by_total, 0.0, 0.1);
    let middle_half = band_mean(&rates_by_total, 0.25, 0.75);
    let top_decile = band_mean(&rates_by_total, 0.9, 1.0);
    let rho = spearman(&totals, &win_rates);

    let mut by_rate: Vec<usize> = (0..ids.len()).collect();
    by_rate.sort_by(|a, b| win_rates[*a].total_cmp(&win_rates[*b]));
    let describe = |indices: &[usize]| {
        indices
            .iter()
            .map(|index| {
                format!(
                    "{} {:.2}",
                    species(ids[*index]).expect("exists").name,
                    win_rates[*index]
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };

    // Random 2v2 teams, Lv 57 against Lv 10, with the species mirrored and the
    // higher level both challenging and defending.
    let mut high_level_points = 0.0;
    for _ in 0..LEVEL_GAP_SAMPLES {
        let [a, b, c, d] = [(); 4].map(|_| ids[rng.index(ids.len())]);
        let (high_ab, low_ab) = (team(&[(a, 57), (b, 57)]), team(&[(a, 10), (b, 10)]));
        let (high_cd, low_cd) = (team(&[(c, 57), (d, 57)]), team(&[(c, 10), (d, 10)]));
        for (high, low) in [(&high_ab, &low_cd), (&high_cd, &low_ab)] {
            high_level_points += points(
                resolve(high, low, next_seed(&mut rng)).winner,
                BattleSide::Challenger,
            );
            high_level_points += points(
                resolve(low, high, next_seed(&mut rng)).winner,
                BattleSide::Defender,
            );
        }
    }
    let level_gap = high_level_points / (4 * LEVEL_GAP_SAMPLES) as f64;

    println!(
        "balance over {} species x {OPPONENTS_PER_SPECIES} opponents at Lv 30: \
         bottom BST decile {bottom_decile:.3}, middle half {middle_half:.3}, top decile {top_decile:.3}, \
         spearman(BST, win rate) {rho:.3}",
        ids.len()
    );
    println!("weakest: {}", describe(&by_rate[..5]));
    println!("strongest: {}", describe(&by_rate[by_rate.len() - 5..]));
    println!(
        "Lv 57 vs Lv 10 random 2v2 over {} battles: higher level scores {level_gap:.3}",
        4 * LEVEL_GAP_SAMPLES
    );
    println!("balance test took {:.2?}", started.elapsed());

    assert!(
        (0.20..=0.40).contains(&bottom_decile),
        "bottom decile {bottom_decile}"
    );
    assert!(
        (0.60..=0.85).contains(&top_decile),
        "top decile {top_decile}"
    );
    assert!((0.55..=0.75).contains(&level_gap), "level gap {level_gap}");
}

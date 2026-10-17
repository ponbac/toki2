use super::*;

async fn get(app: &Router, cookie: &str, uri: &str) -> (StatusCode, Value) {
    send_to(app, cookie, Method::GET, uri, None).await
}

async fn post(app: &Router, cookie: &str, uri: &str, body: Value) -> (StatusCode, Value) {
    send_to(app, cookie, Method::POST, uri, Some(&body.to_string())).await
}

/// A signed-in user whose buddy, of species `lead`, is switched on.
async fn player(app: &Router, pool: &PgPool, full_name: &str, lead: i64) -> (String, i32) {
    let email = format!("{}@example.com", full_name.to_lowercase().replace(' ', "."));
    let (cookie, id) = signed_in_user(app, pool, &email, full_name).await;
    let (status, body) = put_profile(
        app,
        &cookie,
        json!({ "enabled": true, "leadSpeciesId": lead }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    (cookie, id)
}

fn keys(value: &Value) -> Vec<&str> {
    let mut keys: Vec<&str> = value
        .as_object()
        .unwrap_or_else(|| panic!("an object: {value}"))
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    keys
}

/// The result for the challenger, from the last event's winner.
fn challenger_result(battle: &Value) -> &'static str {
    let end = battle["events"].as_array().unwrap().last().unwrap();
    assert_eq!(end["type"], "end", "{battle}");
    match end["winner"].as_str() {
        Some("challenger") => "win",
        Some("defender") => "loss",
        None => "draw",
        Some(other) => panic!("unknown winner {other}"),
    }
}

fn flipped(result: &str) -> &'static str {
    match result {
        "win" => "loss",
        "loss" => "win",
        _ => "draw",
    }
}

#[sqlx::test]
async fn a_colleague_battle_returns_its_replay_and_both_sides_can_open_it(pool: PgPool) {
    let app = app(&pool, 0.0);
    let (ada, ada_id) = player(&app, &pool, "Ada Lovelace", 25).await;
    let (anna, anna_id) = player(&app, &pool, "Anna Lind", 7).await;
    let (bertil, bertil_id) = player(&app, &pool, "Bertil Ek", 1).await;

    assert_eq!(
        get(&app, &ada, "/buddy/opponents").await,
        (
            StatusCode::OK,
            json!({
                "nextBattleAt": null,
                "opponents": [
                    {
                        "userId": anna_id,
                        "fullName": "Anna Lind",
                        "team": [{ "speciesId": 7, "level": 1 }],
                        "record": { "wins": 0, "losses": 0, "draws": 0 },
                        "battledToday": false,
                    },
                    {
                        "userId": bertil_id,
                        "fullName": "Bertil Ek",
                        "team": [{ "speciesId": 1, "level": 1 }],
                        "record": { "wins": 0, "losses": 0, "draws": 0 },
                        "battledToday": false,
                    },
                ],
            })
        )
    );

    let (status, battle) = post(
        &app,
        &ada,
        "/buddy/battles",
        json!({ "opponentId": anna_id }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{battle}");
    assert_eq!(
        keys(&battle),
        vec![
            "challenger",
            "createdAt",
            "defender",
            "engineVersion",
            "events",
            "id",
            "kind",
            "myTeam",
            "opponent",
            "result",
            "role",
            "seed",
            "seen",
            "theirTeam",
        ]
    );
    let result = challenger_result(&battle);
    assert_eq!(battle["kind"], "colleague");
    assert_eq!(battle["role"], "challenger");
    assert_eq!(battle["result"], result);
    assert_eq!(battle["seen"], true);
    assert_eq!(battle["engineVersion"], 1);
    assert_eq!(
        battle["opponent"],
        json!({ "userId": anna_id, "name": "Anna Lind" })
    );
    assert_eq!(battle["myTeam"], json!([{ "speciesId": 25, "level": 1 }]));
    assert_eq!(battle["theirTeam"], json!([{ "speciesId": 7, "level": 1 }]));
    let seed = battle["seed"].as_str().unwrap();
    assert!(
        seed.len() == 8
            && seed
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f')),
        "{seed}"
    );
    assert_eq!(battle["challenger"]["userId"], ada_id);
    assert_eq!(battle["challenger"]["name"], "Ada Lovelace");
    assert_eq!(battle["defender"]["userId"], anna_id);
    let pikachu = &battle["challenger"]["team"][0];
    assert_eq!(
        keys(pikachu),
        vec!["level", "maxHp", "moves", "name", "speciesId", "types"]
    );
    assert_eq!(pikachu["name"], "Pikachu");
    assert_eq!(pikachu["types"], json!(["Electric"]));
    assert_eq!(
        keys(&pikachu["moves"][0]),
        vec!["accuracy", "category", "name", "power", "pp", "type"]
    );
    assert_eq!(
        battle["events"][0],
        json!({ "type": "send", "side": "defender", "index": 0 })
    );

    // The defender opens the same battle from their side.
    let id = battle["id"].as_i64().unwrap();
    let (status, defended) = get(&app, &anna, &format!("/buddy/battles/{id}")).await;
    assert_eq!(status, StatusCode::OK, "{defended}");
    assert_eq!(defended["role"], "defender");
    assert_eq!(defended["result"], flipped(result));
    assert_eq!(defended["seen"], false);
    assert_eq!(
        defended["opponent"],
        json!({ "userId": ada_id, "name": "Ada Lovelace" })
    );
    assert_eq!(defended["myTeam"], battle["theirTeam"]);
    assert_eq!(defended["events"], battle["events"]);
    assert_eq!(defended["seed"], battle["seed"]);

    // Others neither see the battle nor mark it.
    assert_error(
        get(&app, &bertil, &format!("/buddy/battles/{id}")).await,
        StatusCode::NOT_FOUND,
        "a battle someone else fought",
    );
    assert_error(
        post(
            &app,
            &bertil,
            &format!("/buddy/battles/{id}/seen"),
            json!({}),
        )
        .await,
        StatusCode::FORBIDDEN,
        "marking someone else's battle",
    );
    assert_eq!(
        post(&app, &anna, &format!("/buddy/battles/{id}/seen"), json!({})).await,
        (StatusCode::NO_CONTENT, Value::Null)
    );

    let (status, listed) = get(&app, &anna, "/buddy/battles").await;
    assert_eq!(status, StatusCode::OK, "{listed}");
    assert_eq!(keys(&listed), vec!["battles"]);
    let summary = &listed["battles"][0];
    assert_eq!(
        keys(summary),
        vec![
            "createdAt",
            "id",
            "kind",
            "myTeam",
            "opponent",
            "result",
            "role",
            "seen",
            "theirTeam",
        ]
    );
    assert_eq!(summary["id"], id);
    assert_eq!(summary["role"], "defender");
    assert_eq!(summary["seen"], true);
    let (_, mine) = get(&app, &ada, "/buddy/battles").await;
    assert_eq!(mine["battles"][0]["role"], "challenger");
    assert_eq!(mine["battles"][0]["result"], result);
    let (_, nothing) = get(&app, &bertil, "/buddy/battles").await;
    assert_eq!(nothing, json!({ "battles": [] }));

    // Starting the battle earned Ada 20 XP.
    let (_, profile) = get_profile(&app, &ada).await;
    assert_eq!(profile["profile"]["today"]["battles"], 1);
    assert_eq!(profile["profile"]["today"]["battleXp"], 20);
    assert_eq!(profile["profile"]["buddies"][0]["xp"], 20);

    for (method, uri, expected) in [
        (Method::GET, "/buddy/battles/first", StatusCode::BAD_REQUEST),
        (Method::GET, "/buddy/battles/999999", StatusCode::NOT_FOUND),
        (
            Method::POST,
            "/buddy/battles/999999/seen",
            StatusCode::NOT_FOUND,
        ),
    ] {
        assert_error(send_to(&app, &ada, method, uri, None).await, expected, uri);
    }
}

#[sqlx::test]
async fn broken_battle_rules_are_conflicts_and_the_hourly_limit_is_too_many_requests(pool: PgPool) {
    let app = app(&pool, 0.0);
    let (ada, ada_id) = player(&app, &pool, "Ada Lovelace", 25).await;
    let (_, anna_id) = player(&app, &pool, "Anna Lind", 7).await;
    let (_, bertil_id) = player(&app, &pool, "Bertil Ek", 1).await;
    let (dora, dora_id) = player(&app, &pool, "Dora Off", 4).await;
    let (status, _) = put_profile(&app, &dora, json!({ "enabled": false })).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = post(
        &app,
        &ada,
        "/buddy/battles",
        json!({ "opponentId": anna_id }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_error(
        post(
            &app,
            &ada,
            "/buddy/battles",
            json!({ "opponentId": anna_id }),
        )
        .await,
        StatusCode::CONFLICT,
        "the same colleague twice a day",
    );
    let (status, opponents) = get(&app, &ada, "/buddy/opponents").await;
    assert_eq!(status, StatusCode::OK);
    assert!(opponents["nextBattleAt"].is_string(), "{opponents}");
    assert_eq!(opponents["opponents"][0]["battledToday"], true);
    let (status, cooldown) = post(
        &app,
        &ada,
        "/buddy/battles",
        json!({ "opponentId": bertil_id }),
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "{cooldown}");
    assert_eq!(
        cooldown["error"],
        "you can start your next colleague battle in 60 minutes"
    );

    for (body, expected, case) in [
        (
            json!({ "opponentId": dora_id }),
            StatusCode::CONFLICT,
            "switched off",
        ),
        (
            json!({ "opponentId": 999_999 }),
            StatusCode::CONFLICT,
            "nobody",
        ),
        (
            json!({ "opponentId": ada_id }),
            StatusCode::BAD_REQUEST,
            "oneself",
        ),
        (
            json!({ "opponentId": "7" }),
            StatusCode::BAD_REQUEST,
            "a string id",
        ),
        (
            json!({ "opponentId": 3_000_000_000_i64 }),
            StatusCode::BAD_REQUEST,
            "an id out of range",
        ),
        (json!({}), StatusCode::BAD_REQUEST, "no opponent"),
        (
            json!({ "opponentId": anna_id, "seed": 1 }),
            StatusCode::BAD_REQUEST,
            "an unknown field",
        ),
    ] {
        assert_error(
            post(&app, &ada, "/buddy/battles", body).await,
            expected,
            case,
        );
    }

    // Without a buddy, or with it switched off, there is nothing to battle with.
    let stranger = test_user(&app, &pool, "stranger@example.com").await;
    for cookie in [&stranger, &dora] {
        for (method, uri) in [
            (Method::GET, "/buddy/opponents"),
            (Method::GET, "/buddy/trainer"),
            (Method::POST, "/buddy/trainer/battles"),
        ] {
            assert_error(
                send_to(&app, cookie, method, uri, None).await,
                StatusCode::CONFLICT,
                uri,
            );
        }
        assert_error(
            post(
                &app,
                cookie,
                "/buddy/battles",
                json!({ "opponentId": anna_id }),
            )
            .await,
            StatusCode::CONFLICT,
            "a colleague battle",
        );
    }
}

#[sqlx::test]
async fn the_daily_trainer_can_be_fought_once_a_day(pool: PgPool) {
    let app = app(&pool, 0.0);
    let (ada, _) = player(&app, &pool, "Ada Lovelace", 25).await;
    let today = local_today(&pool).await;

    let (status, trainer) = get(&app, &ada, "/buddy/trainer").await;
    assert_eq!(status, StatusCode::OK, "{trainer}");
    assert_eq!(
        keys(&trainer),
        vec!["battleId", "date", "name", "result", "team"]
    );
    assert_eq!(trainer["date"], today);
    assert_eq!(trainer["battleId"], Value::Null);
    assert_eq!(trainer["result"], Value::Null);
    let name = trainer["name"].as_str().unwrap();
    assert!(!name.is_empty());
    // Ada has no partner, so the trainer's lead fights alone.
    let team = trainer["team"].as_array().unwrap();
    assert_eq!(team.len(), 1);
    assert_eq!(team[0]["level"], 25);

    let (status, battle) = post(&app, &ada, "/buddy/trainer/battles", json!({})).await;
    assert_eq!(status, StatusCode::CREATED, "{battle}");
    assert_eq!(battle["kind"], "trainer");
    assert_eq!(battle["role"], "challenger");
    assert_eq!(battle["opponent"], json!({ "userId": null, "name": name }));
    assert_eq!(battle["defender"]["userId"], Value::Null);
    assert_eq!(battle["defender"]["name"], name);
    assert_eq!(battle["theirTeam"], trainer["team"]);
    assert_eq!(battle["seen"], true);

    assert_error(
        send_to(&app, &ada, Method::POST, "/buddy/trainer/battles", None).await,
        StatusCode::CONFLICT,
        "the trainer twice a day",
    );
    let (_, fought) = get(&app, &ada, "/buddy/trainer").await;
    assert_eq!(fought["battleId"], battle["id"]);
    assert_eq!(fought["result"], challenger_result(&battle));
    // The trainer does not hold back colleague battles.
    let (_, opponents) = get(&app, &ada, "/buddy/opponents").await;
    assert_eq!(opponents["nextBattleAt"], Value::Null);
}

#[sqlx::test]
async fn the_leaderboard_counts_the_battles_started_in_a_month(pool: PgPool) {
    let app = app(&pool, 0.0);
    let (ada, ada_id) = player(&app, &pool, "Ada Lovelace", 25).await;
    let (anna, anna_id) = player(&app, &pool, "Anna Lind", 7).await;
    let (_, battle) = post(
        &app,
        &ada,
        "/buddy/battles",
        json!({ "opponentId": anna_id }),
    )
    .await;
    let result = challenger_result(&battle);
    let month = local_today(&pool).await[..7].to_string();

    let (status, leaderboard) = get(&app, &anna, "/buddy/leaderboard").await;

    assert_eq!(status, StatusCode::OK, "{leaderboard}");
    // Ada's 20 XP is Lv 4; Anna only defended.
    assert_eq!(
        leaderboard,
        json!({
            "month": month,
            "rows": [{
                "userId": ada_id,
                "fullName": "Ada Lovelace",
                "team": [{ "speciesId": 25, "level": 4 }],
                "wins": i64::from(result == "win"),
                "losses": i64::from(result == "loss"),
                "draws": i64::from(result == "draw"),
                "trainerWins": 0,
            }],
        })
    );
    assert_eq!(
        get(&app, &anna, &format!("/buddy/leaderboard?month={month}")).await,
        (StatusCode::OK, leaderboard)
    );
    assert_eq!(
        get(&app, &anna, "/buddy/leaderboard?month=2026-01").await,
        (StatusCode::OK, json!({ "month": "2026-01", "rows": [] }))
    );
    for query in [
        "month=2026-13",
        "month=oct",
        "month=2026-1",
        "month=2026-10-01",
        "from=2026-10",
    ] {
        assert_error(
            get(&app, &anna, &format!("/buddy/leaderboard?{query}")).await,
            StatusCode::BAD_REQUEST,
            query,
        );
    }
}

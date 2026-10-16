use std::{collections::HashMap, sync::Mutex};

use sqlx::PgPool;
use web_push::WebPushMessage;

use super::*;
use crate::{domain::models::UserId, repositories::NewPushSubscription};

// A browser's P-256 public key and auth secret, generated for these tests, so
// messages encrypt as they would for a real subscription.
const P256DH: &str =
    "BMNhNnWR6YXUgG36xpYxZAHsFQBoOgS5y_M1_QmmuYaoQN_tjoTiojgNFm-0VTkmNQg6Euo58Gx0OISl0OxH7lo";
const AUTH: &str = "7yePX-w_mWZjkyt2nv8JdA";

/// A device that subscribes again while a notice is sent to it.
struct Resubscription {
    subscriptions: PushSubscriptionRepositoryImpl,
    user_id: UserId,
    device: String,
    endpoint: String,
}

/// A push service that answers each endpoint as told, after `delay`, and records the
/// endpoints it was sent to.
struct FakePushService {
    answers: HashMap<String, WebPushError>,
    delay: Duration,
    sent: Mutex<Vec<String>>,
    resubscription: Option<Resubscription>,
}

impl FakePushService {
    fn new(answers: &[(&str, WebPushError)], delay: Duration) -> Self {
        Self {
            answers: answers
                .iter()
                .map(|(endpoint, error)| (endpoint.to_string(), error.clone()))
                .collect(),
            delay,
            sent: Mutex::new(Vec::new()),
            resubscription: None,
        }
    }

    /// Before answering, `device` of `user_id` subscribes again with `endpoint`, the
    /// way the app does, which keeps the subscription's row.
    fn resubscribing(
        mut self,
        pool: &PgPool,
        user_id: UserId,
        device: &str,
        endpoint: &str,
    ) -> Self {
        let db = sqlx_tracing::PoolBuilder::from(pool.clone()).build();
        self.resubscription = Some(Resubscription {
            subscriptions: PushSubscriptionRepositoryImpl::new(db),
            user_id,
            device: device.to_string(),
            endpoint: endpoint.to_string(),
        });
        self
    }

    fn sent(&self) -> Vec<String> {
        let mut sent = self.sent.lock().unwrap().clone();
        sent.sort();
        sent
    }
}

#[async_trait]
impl WebPushClient for FakePushService {
    async fn send(&self, message: WebPushMessage) -> Result<(), WebPushError> {
        let endpoint = message.endpoint.to_string();
        self.sent.lock().unwrap().push(endpoint.clone());
        if let Some(again) = &self.resubscription {
            again
                .subscriptions
                .upsert_push_subscription(NewPushSubscription {
                    user_id: again.user_id.as_i32(),
                    device: again.device.clone(),
                    endpoint: again.endpoint.clone(),
                    auth: AUTH.to_string(),
                    p256dh: P256DH.to_string(),
                })
                .await
                .unwrap();
        }
        tokio::time::sleep(self.delay).await;
        match self.answers.get(&endpoint) {
            Some(error) => Err(error.clone()),
            None => Ok(()),
        }
    }
}

fn notifier(
    pool: &PgPool,
    service: FakePushService,
    timeout: Duration,
) -> WebPushBuddyNotifier<FakePushService> {
    let db = sqlx_tracing::PoolBuilder::from(pool.clone()).build();
    WebPushBuddyNotifier {
        subscriptions: PushSubscriptionRepositoryImpl::new(db),
        client: service,
        app_url: Url::parse("https://toki.example.com").unwrap(),
        timeout,
    }
}

async fn insert_user(pool: &PgPool, email: &str) -> UserId {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, full_name, picture, access_token)
         VALUES ($1, 'Test User', '', '')
         RETURNING id",
    )
    .bind(email)
    .fetch_one(pool)
    .await
    .unwrap();
    UserId::new(id)
}

async fn subscribe(pool: &PgPool, user_id: UserId, device: &str, endpoint: &str) {
    sqlx::query(
        "INSERT INTO push_subscriptions (user_id, device, endpoint, auth, p256dh)
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(user_id.as_i32())
    .bind(device)
    .bind(endpoint)
    .bind(AUTH)
    .bind(P256DH)
    .execute(pool)
    .await
    .unwrap();
}

async fn endpoints(pool: &PgPool) -> Vec<String> {
    sqlx::query_scalar("SELECT endpoint FROM push_subscriptions ORDER BY endpoint")
        .fetch_all(pool)
        .await
        .unwrap()
}

fn notice(defender_id: UserId, defender_buddies: &[&str], result: BattleResult) -> BattleNotice {
    BattleNotice {
        battle_id: BattleId::new(42),
        defender_id,
        challenger_name: "Anna Lind".to_string(),
        defender_buddies: defender_buddies.iter().map(ToString::to_string).collect(),
        result,
    }
}

#[sqlx::test]
async fn a_notice_reaches_every_device_and_gone_subscriptions_are_deleted(pool: PgPool) {
    let defender = insert_user(&pool, "dev@example.com").await;
    let colleague = insert_user(&pool, "colleague@example.com").await;
    let (works, expired, unsubscribed, flaky, elsewhere) = (
        "https://push.example.com/works",
        "https://push.example.com/expired",
        "https://push.example.com/unsubscribed",
        "https://push.example.com/flaky",
        "https://push.example.com/elsewhere",
    );
    subscribe(&pool, defender, "laptop", works).await;
    subscribe(&pool, defender, "phone", expired).await;
    subscribe(&pool, defender, "tablet", unsubscribed).await;
    subscribe(&pool, defender, "desktop", flaky).await;
    subscribe(&pool, colleague, "laptop", elsewhere).await;
    let service = FakePushService::new(
        &[
            (expired, WebPushError::EndpointNotFound),
            (unsubscribed, WebPushError::EndpointNotValid),
            (flaky, WebPushError::ServerError(None)),
            (elsewhere, WebPushError::EndpointNotFound),
        ],
        Duration::ZERO,
    );
    let notifier = notifier(&pool, service, SEND_TIMEOUT);

    notifier
        .battle_fought(&notice(defender, &["Pikachu"], BattleResult::Win))
        .await;

    assert_eq!(
        notifier.client.sent(),
        vec![expired, flaky, unsubscribed, works]
    );
    // A failure that may pass keeps the subscription; others' devices are untouched.
    assert_eq!(endpoints(&pool).await, vec![elsewhere, flaky, works]);
}

#[sqlx::test]
async fn a_device_that_subscribed_again_while_its_old_subscription_failed_keeps_the_new_one(
    pool: PgPool,
) {
    let defender = insert_user(&pool, "dev@example.com").await;
    let (expired, renewed) = (
        "https://push.example.com/expired",
        "https://push.example.com/renewed",
    );
    subscribe(&pool, defender, "phone", expired).await;
    let service =
        FakePushService::new(&[(expired, WebPushError::EndpointNotFound)], Duration::ZERO)
            .resubscribing(&pool, defender, "phone", renewed);
    let notifier = notifier(&pool, service, SEND_TIMEOUT);

    notifier
        .battle_fought(&notice(defender, &["Pikachu"], BattleResult::Win))
        .await;

    assert_eq!(notifier.client.sent(), vec![expired]);
    assert_eq!(endpoints(&pool).await, vec![renewed]);
}

#[sqlx::test]
async fn a_push_service_that_does_not_answer_is_given_up_on(pool: PgPool) {
    let defender = insert_user(&pool, "dev@example.com").await;
    let slow = "https://push.example.com/slow";
    subscribe(&pool, defender, "laptop", slow).await;
    let notifier = notifier(
        &pool,
        FakePushService::new(
            &[(slow, WebPushError::EndpointNotFound)],
            Duration::from_secs(30),
        ),
        Duration::from_millis(50),
    );

    tokio::time::timeout(
        Duration::from_secs(5),
        notifier.battle_fought(&notice(defender, &["Pikachu"], BattleResult::Loss)),
    )
    .await
    .expect("the notice gives up after its timeout");

    assert_eq!(notifier.client.sent(), vec![slow]);
    assert_eq!(endpoints(&pool).await, vec![slow]);
}

#[test]
fn a_notice_links_to_the_replay_and_says_how_the_defenders_buddies_did() {
    let defender = UserId::new(7);

    for app_url in ["https://toki.example.com", "https://toki.example.com/"] {
        assert_eq!(
            battle_url(&Url::parse(app_url).unwrap(), BattleId::new(42)),
            "https://toki.example.com/buddies/battles/42"
        );
    }
    assert_eq!(
        battle_title(&notice(defender, &["Pikachu"], BattleResult::Win)),
        "Anna Lind battled your buddies"
    );
    for (buddies, result, body) in [
        (
            &["Pikachu", "Bulbasaur"][..],
            BattleResult::Win,
            "Your Pikachu and Bulbasaur won!",
        ),
        (
            &["Pikachu", "Bulbasaur"][..],
            BattleResult::Loss,
            "Your Pikachu and Bulbasaur lost.",
        ),
        (&["Pikachu"][..], BattleResult::Win, "Your Pikachu won!"),
        (&["Pikachu"][..], BattleResult::Loss, "Your Pikachu lost."),
        (
            &["Pikachu", "Bulbasaur"][..],
            BattleResult::Draw,
            "It was a draw.",
        ),
    ] {
        assert_eq!(battle_body(&notice(defender, buddies, result)), body);
    }
}

use std::{
    sync::{Mutex, OnceLock},
    time::Duration,
};

use opentelemetry::{
    global,
    metrics::{Counter, Gauge, Histogram, ObservableGauge, UpDownCounter},
    KeyValue,
};
use sqlx::PgPool;

use crate::domain::DbNotificationType;

static METRICS: OnceLock<Metrics> = OnceLock::new();

pub(crate) fn init() {
    let _ = metrics();
}

pub(crate) fn register_db_pool(pool: PgPool) {
    if let Some(metrics) = METRICS.get() {
        metrics.register_db_pool(pool);
    }
}

pub(crate) fn record_http_request(method: &str, route: &str, status: u16, duration: Duration) {
    if let Some(metrics) = METRICS.get() {
        metrics.http_request_duration.record(
            duration.as_secs_f64(),
            &http_request_attributes(method, route, status),
        );
    }
}

pub(crate) fn record_http_active_request_delta(method: &str, route: &str, delta: i64) {
    if let Some(metrics) = METRICS.get() {
        metrics
            .http_active_requests
            .add(delta, &http_active_request_attributes(method, route));
    }
}

pub(crate) fn record_repo_differ_poll(
    repo_key: &str,
    trigger: &str,
    result: &'static str,
    duration: Duration,
    open_pull_requests: Option<u64>,
    changed_pull_requests: Option<u64>,
) {
    if let Some(metrics) = METRICS.get() {
        let attrs = repo_differ_poll_attributes(repo_key, trigger, result);
        metrics
            .repo_differ_poll_duration
            .record(duration.as_secs_f64(), &attrs);
        metrics.repo_differ_polls.add(1, &attrs);

        if let Some(open_pull_requests) = open_pull_requests {
            metrics.repo_differ_open_pull_requests.record(
                open_pull_requests,
                &[KeyValue::new("repo.key", repo_key.to_string())],
            );
        }

        if let Some(changed_pull_requests) = changed_pull_requests {
            metrics.repo_differ_changed_pull_requests.record(
                changed_pull_requests,
                &[
                    KeyValue::new("repo.key", repo_key.to_string()),
                    KeyValue::new("trigger", trigger.to_string()),
                ],
            );
        }
    }
}

pub(crate) fn record_notification_created(notification_type: DbNotificationType) {
    if let Some(metrics) = METRICS.get() {
        metrics.notifications_created.add(
            1,
            &[KeyValue::new(
                "notification.type",
                notification_type_label(notification_type),
            )],
        );
    }
}

pub(crate) fn record_push_notifications(attempted: u64, sent: u64, failed: u64) {
    if let Some(metrics) = METRICS.get() {
        if attempted > 0 {
            metrics.notifications_push_attempted.add(attempted, &[]);
        }
        if sent > 0 {
            metrics.notifications_push_sent.add(sent, &[]);
        }
        if failed > 0 {
            metrics.notifications_push_failed.add(failed, &[]);
        }
    }
}

struct Metrics {
    http_request_duration: Histogram<f64>,
    http_active_requests: UpDownCounter<i64>,
    repo_differ_poll_duration: Histogram<f64>,
    repo_differ_polls: Counter<u64>,
    repo_differ_open_pull_requests: Gauge<u64>,
    repo_differ_changed_pull_requests: Gauge<u64>,
    notifications_created: Counter<u64>,
    notifications_push_attempted: Counter<u64>,
    notifications_push_sent: Counter<u64>,
    notifications_push_failed: Counter<u64>,
    db_pool_gauges: Mutex<Vec<ObservableGauge<u64>>>,
}

impl Metrics {
    fn new() -> Self {
        let meter = global::meter("toki-api");

        Self {
            http_request_duration: meter
                .f64_histogram("http.server.request.duration")
                .with_unit("s")
                .build(),
            http_active_requests: meter
                .i64_up_down_counter("http.server.active_requests")
                .with_unit("{request}")
                .build(),
            repo_differ_poll_duration: meter
                .f64_histogram("toki.repo_differ.poll.duration")
                .with_unit("s")
                .build(),
            repo_differ_polls: meter
                .u64_counter("toki.repo_differ.polls")
                .with_unit("{poll}")
                .build(),
            repo_differ_open_pull_requests: meter
                .u64_gauge("toki.repo_differ.open_pull_requests")
                .with_unit("{pull_request}")
                .build(),
            repo_differ_changed_pull_requests: meter
                .u64_gauge("toki.repo_differ.changed_pull_requests")
                .with_unit("{pull_request}")
                .build(),
            notifications_created: meter
                .u64_counter("toki.notifications.created")
                .with_unit("{notification}")
                .build(),
            notifications_push_attempted: meter
                .u64_counter("toki.notifications.push.attempted")
                .with_unit("{notification}")
                .build(),
            notifications_push_sent: meter
                .u64_counter("toki.notifications.push.sent")
                .with_unit("{notification}")
                .build(),
            notifications_push_failed: meter
                .u64_counter("toki.notifications.push.failed")
                .with_unit("{notification}")
                .build(),
            db_pool_gauges: Mutex::new(Vec::new()),
        }
    }

    fn register_db_pool(&self, pool: PgPool) {
        let meter = global::meter("toki-api");
        let connections = meter
            .u64_observable_gauge("toki.db.pool.connections")
            .with_unit("{connection}")
            .with_callback(move |observer| {
                let snapshot = db_pool_connection_snapshot(pool.size(), pool.num_idle());
                observer.observe(snapshot.open as u64, &[KeyValue::new("state", "open")]);
                observer.observe(snapshot.idle as u64, &[KeyValue::new("state", "idle")]);
                observer.observe(snapshot.used as u64, &[KeyValue::new("state", "used")]);
                observer.observe(
                    pool.options().get_max_connections() as u64,
                    &[KeyValue::new("state", "max")],
                );
            })
            .build();

        if let Ok(mut gauges) = self.db_pool_gauges.lock() {
            gauges.push(connections);
        }
    }
}

fn metrics() -> &'static Metrics {
    METRICS.get_or_init(Metrics::new)
}

fn http_request_attributes(method: &str, route: &str, status: u16) -> Vec<KeyValue> {
    let mut attrs = http_active_request_attributes(method, route);
    attrs.push(KeyValue::new(
        "http.response.status_code",
        i64::from(status),
    ));
    attrs
}

fn http_active_request_attributes(method: &str, route: &str) -> Vec<KeyValue> {
    vec![
        KeyValue::new("http.request.method", method.to_string()),
        KeyValue::new("http.route", route.to_string()),
    ]
}

fn repo_differ_poll_attributes(
    repo_key: &str,
    trigger: &str,
    result: &'static str,
) -> Vec<KeyValue> {
    vec![
        KeyValue::new("repo.key", repo_key.to_string()),
        KeyValue::new("trigger", trigger.to_string()),
        KeyValue::new("result", result),
    ]
}

fn notification_type_label(notification_type: DbNotificationType) -> &'static str {
    match notification_type {
        DbNotificationType::PrClosed => "pr_closed",
        DbNotificationType::ThreadAdded => "thread_added",
        DbNotificationType::ThreadUpdated => "thread_updated",
        DbNotificationType::CommentMentioned => "comment_mentioned",
    }
}

#[derive(Debug, PartialEq, Eq)]
struct DbPoolConnectionSnapshot {
    open: u32,
    idle: usize,
    used: u32,
}

fn db_pool_connection_snapshot(open: u32, idle: usize) -> DbPoolConnectionSnapshot {
    DbPoolConnectionSnapshot {
        open,
        idle,
        used: open.saturating_sub(idle as u32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notification_type_mapping_returns_stable_snake_case_values() {
        assert_eq!(
            notification_type_label(DbNotificationType::PrClosed),
            "pr_closed"
        );
        assert_eq!(
            notification_type_label(DbNotificationType::ThreadAdded),
            "thread_added"
        );
        assert_eq!(
            notification_type_label(DbNotificationType::ThreadUpdated),
            "thread_updated"
        );
        assert_eq!(
            notification_type_label(DbNotificationType::CommentMentioned),
            "comment_mentioned"
        );
    }

    #[test]
    fn db_pool_used_calculation_saturates_when_idle_exceeds_open() {
        assert_eq!(
            db_pool_connection_snapshot(2, 5),
            DbPoolConnectionSnapshot {
                open: 2,
                idle: 5,
                used: 0
            }
        );
    }

    #[test]
    fn http_metric_attributes_use_route_templates_and_allowed_keys() {
        let attrs = http_request_attributes("GET", "/repos/:repo_id/pulls", 200);
        let keys = attrs
            .iter()
            .map(|attr| attr.key.as_str())
            .collect::<Vec<_>>();

        assert_eq!(
            keys,
            vec![
                "http.request.method",
                "http.route",
                "http.response.status_code"
            ]
        );
        assert!(attrs.iter().any(|attr| attr.key.as_str() == "http.route"
            && attr.value.as_str() == "/repos/:repo_id/pulls"));
        assert!(!keys.contains(&"user.id"));
        assert!(!keys.contains(&"url.path"));
        assert!(!keys.contains(&"url.query"));
    }
}

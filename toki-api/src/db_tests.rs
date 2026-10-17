use std::sync::{Arc, Mutex};

use futures::TryStreamExt;
use opentelemetry::trace::{SpanKind, Status, TracerProvider};
use opentelemetry_sdk::{
    error::OTelSdkResult,
    trace::{SdkTracerProvider, SpanData, SpanExporter},
};
use sqlx::{postgres::PgPoolOptions, Error, Executor, PgPool, Statement};
use tracing::{instrument::WithSubscriber, Instrument};
use tracing_subscriber::layer::SubscriberExt;

use super::DbPool;

// The wrapper owns the SQLx integration boundary. These tests protect query
// reconstruction, transaction lifetime, and exported traces rather than merely
// checking that PostgreSQL implements transactions.
#[sqlx::test(migrations = false)]
async fn preserves_bound_queries_and_transaction_lifetime(pool: PgPool) {
    let single_connection_pool = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let db = DbPool::from(single_connection_pool.clone());

    sqlx::raw_sql(
        "CREATE TABLE db_wrapper_contract (value text NOT NULL); \
         INSERT INTO db_wrapper_contract VALUES ('seed')",
    )
    .execute(&db)
    .await
    .unwrap();

    let statement = (&db)
        .prepare(sqlx::SqlStr::from_static("SELECT $1::text"))
        .await
        .unwrap();
    let bound: String = statement
        .query_scalar()
        .bind("prepared-bound-value")
        .persistent(false)
        .fetch_one(&db)
        .await
        .unwrap();
    assert_eq!(bound, "prepared-bound-value");

    let mut transaction = db.begin().await.unwrap();
    let affected = sqlx::query("INSERT INTO db_wrapper_contract VALUES ($1)")
        .bind("committed")
        .execute(&mut transaction.executor())
        .await
        .unwrap();
    assert_eq!(affected.rows_affected(), 1);
    transaction.commit().await.unwrap();

    let mut transaction = db.begin().await.unwrap();
    sqlx::query("INSERT INTO db_wrapper_contract VALUES ($1)")
        .bind("dropped")
        .execute(&mut transaction.executor())
        .await
        .unwrap();
    drop(transaction);

    // With only one connection, this also proves the dropped transaction
    // released its connection and finished its queued rollback before reuse.
    let values: Vec<String> = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        sqlx::query_scalar("SELECT value FROM db_wrapper_contract ORDER BY value")
            .fetch_all(&db.clone()),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(values, vec!["committed", "seed"]);
    single_connection_pool.close().await;
}

#[derive(Clone, Debug, Default)]
struct CapturedSpans {
    spans: Arc<Mutex<Vec<SpanData>>>,
    exported: Arc<tokio::sync::Notify>,
}

impl SpanExporter for CapturedSpans {
    async fn export(&self, spans: Vec<SpanData>) -> OTelSdkResult {
        self.spans.lock().unwrap().extend(spans);
        self.exported.notify_one();
        Ok(())
    }
}

fn attribute<'a>(span: &'a SpanData, key: &str) -> Option<&'a opentelemetry::Value> {
    span.attributes
        .iter()
        .find(|attribute| attribute.key.as_str() == key)
        .map(|attribute| &attribute.value)
}

#[sqlx::test(migrations = false)]
async fn exports_query_context_counts_and_errors_without_bind_values(pool: PgPool) {
    let db = DbPool::from(pool.clone());
    let captured = CapturedSpans::default();
    // Tracing's single-dispatcher fast path uses the current thread when a
    // callsite first registers. Keep both dispatchers registered so concurrent
    // untraced tests cannot cache "never" for this test's shared DB callsites.
    let _untraced_dispatch = tracing::Dispatch::new(tracing::subscriber::NoSubscriber::default());
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(captured.clone())
        .build();
    let subscriber = tracing_subscriber::registry()
        .with(tracing_opentelemetry::layer().with_tracer(provider.tracer("db-wrapper-test")));

    async {
        let parent = tracing::info_span!("wrapper-request");
        async {
            let values: Vec<String> =
                sqlx::query_scalar("SELECT $1::text FROM generate_series(1, 3)")
                    .bind("private-bind-value")
                    .fetch_all(&db)
                    .await
                    .unwrap();
            assert_eq!(values, vec!["private-bind-value"; 3]);

            let streamed: Vec<i32> = sqlx::query_scalar::<_, i32>("SELECT generate_series(1, 4)")
                .fetch(&db)
                .try_collect()
                .await
                .unwrap();
            assert_eq!(streamed, vec![1, 2, 3, 4]);

            let missing: Option<i32> = sqlx::query_scalar("SELECT 1 WHERE false")
                .fetch_optional(&db)
                .await
                .unwrap();
            assert!(missing.is_none());
            let missing = sqlx::query("SELECT 2 WHERE false").fetch_one(&db).await;
            assert!(matches!(missing, Err(Error::RowNotFound)));

            let error = sqlx::query("SELECT 1 / 0").execute(&db).await.unwrap_err();
            assert_eq!(
                error.as_database_error().unwrap().code().as_deref(),
                Some("22012")
            );
            let error = sqlx::query("SELECT $1::text::integer")
                .bind("private-failing-bind-value")
                .execute(&db)
                .await
                .unwrap_err();
            assert_eq!(
                error.as_database_error().unwrap().code().as_deref(),
                Some("22P02")
            );
            assert!(error.to_string().contains("private-failing-bind-value"));
        }
        .instrument(parent)
        .await;
    }
    .with_subscriber(subscriber)
    .await;
    // SQLx returns pooled connections in background tasks that inherit query
    // spans. Drain those tasks before inspecting completed exported spans.
    pool.close().await;
    provider.force_flush().unwrap();

    // Connection shutdown can finish before the cleanup task drops its final
    // span handle. Await actual export completion instead of racing that drop.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            let exported = captured.exported.notified();
            if captured.spans.lock().unwrap().len() >= 7 {
                break;
            }
            exported.await;
        }
    })
    .await
    .expect("query and request spans must finish exporting");

    let spans = captured.spans.lock().unwrap();
    let parent = spans
        .iter()
        .find(|span| span.name == "wrapper-request")
        .unwrap();
    let query_spans: Vec<_> = spans
        .iter()
        .filter(|span| span.name.starts_with("sqlx."))
        .collect();
    assert_eq!(query_spans.len(), 6);
    for span in &query_spans {
        assert_eq!(span.span_kind, SpanKind::Client);
        assert_eq!(span.parent_span_id, parent.span_context.span_id());
        assert_eq!(span.span_context.trace_id(), parent.span_context.trace_id());
        assert_eq!(
            attribute(span, "db.system.name"),
            Some(&"postgresql".into())
        );
        assert_eq!(
            attribute(span, "db.namespace"),
            Some(
                &pool
                    .connect_options()
                    .get_database()
                    .unwrap()
                    .to_owned()
                    .into()
            )
        );
        assert_eq!(
            attribute(span, "server.address"),
            Some(&pool.connect_options().get_host().to_owned().into())
        );
        assert_eq!(
            attribute(span, "server.port"),
            Some(&i64::from(pool.connect_options().get_port()).into())
        );
        for secret in ["private-bind-value", "private-failing-bind-value"] {
            assert!(span
                .attributes
                .iter()
                .all(|attribute| !attribute.value.as_str().contains(secret)));
            if let Status::Error { description } = &span.status {
                assert!(!description.contains(secret));
            }
        }
    }

    let by_sql = |sql: &str| {
        *query_spans
            .iter()
            .find(|span| {
                attribute(span, "db.query.text").is_some_and(|value| value.as_str() == sql)
            })
            .unwrap()
    };
    assert_eq!(
        attribute(
            by_sql("SELECT $1::text FROM generate_series(1, 3)"),
            "db.response.returned_rows"
        ),
        Some(&3_i64.into())
    );
    assert_eq!(
        attribute(
            by_sql("SELECT generate_series(1, 4)"),
            "db.response.returned_rows"
        ),
        Some(&4_i64.into())
    );
    assert_eq!(
        attribute(by_sql("SELECT 1 WHERE false"), "db.response.returned_rows"),
        Some(&0_i64.into())
    );
    for sql in ["SELECT 2 WHERE false", "SELECT 1 / 0"] {
        assert!(matches!(by_sql(sql).status, Status::Error { .. }));
        assert!(attribute(by_sql(sql), "error.message").is_some());
    }
    assert_eq!(
        attribute(by_sql("SELECT 1 / 0"), "db.response.status_code"),
        Some(&"22012".into())
    );
}

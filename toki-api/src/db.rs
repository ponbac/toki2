use std::{
    sync::Arc,
    task::{Context, Poll},
};

use futures::{future::BoxFuture, stream::BoxStream};
use sqlx::{
    error::BoxDynError,
    postgres::{PgArguments, PgQueryResult, PgRow, PgStatement, PgTypeInfo},
    Either, Error, Execute, Executor, PgConnection, PgPool, Postgres, SqlStr,
};
use tracing::{field, Instrument, Span};

use crate::config::DatabaseSettings;

/// Keep query instrumentation at the executor boundary so repositories don't
/// need to know how their SQL is traced.
#[derive(Clone, Debug)]
pub struct DbPool {
    inner: PgPool,
    attributes: Arc<Attributes>,
}

#[derive(Debug)]
struct Attributes {
    database: Option<String>,
    host: String,
    port: u16,
}

impl From<PgPool> for DbPool {
    fn from(inner: PgPool) -> Self {
        let options = inner.connect_options();
        let attributes = Arc::new(Attributes {
            database: options.get_database().map(str::to_owned),
            host: options.get_host().to_owned(),
            port: options.get_port(),
        });
        Self { inner, attributes }
    }
}

pub(crate) fn traced_pool(inner: PgPool, settings: &DatabaseSettings) -> DbPool {
    DbPool {
        inner,
        attributes: Arc::new(Attributes {
            database: Some(settings.database_name.clone()),
            host: settings.host.clone(),
            port: settings.port,
        }),
    }
}

impl DbPool {
    pub(crate) async fn begin(&self) -> Result<DbTransaction, Error> {
        Ok(DbTransaction {
            inner: self.inner.begin().await?,
            attributes: self.attributes.clone(),
        })
    }
}

#[derive(Debug)]
pub(crate) struct DbTransaction {
    inner: sqlx::Transaction<'static, Postgres>,
    attributes: Arc<Attributes>,
}

impl DbTransaction {
    pub(crate) fn executor(&mut self) -> DbConnection<'_> {
        DbConnection {
            inner: &mut self.inner,
            attributes: &self.attributes,
        }
    }

    pub(crate) async fn commit(self) -> Result<(), Error> {
        self.inner.commit().await
    }
}

#[derive(Debug)]
pub(crate) struct DbConnection<'c> {
    inner: &'c mut PgConnection,
    attributes: &'c Attributes,
}

// SQLx 0.9 consumes Execute when reading its SQL. Preserve the complete query
// before doing that: argument encoding errors, prepared metadata, and the
// distinction between simple queries and queries with an empty argument list.
struct Query {
    sql: SqlStr,
    statement: Option<PgStatement>,
    arguments: Result<Option<PgArguments>, BoxDynError>,
    persistent: bool,
}

impl Query {
    fn new<'q>(mut query: impl Execute<'q, Postgres>) -> Self {
        let statement = query.statement().cloned();
        let persistent = query.persistent();
        let arguments = query.take_arguments();
        Self {
            sql: query.sql(),
            statement,
            arguments,
            persistent,
        }
    }
}

impl Execute<'_, Postgres> for Query {
    fn sql(self) -> SqlStr {
        self.sql
    }

    fn statement(&self) -> Option<&PgStatement> {
        self.statement.as_ref()
    }

    fn take_arguments(&mut self) -> Result<Option<PgArguments>, BoxDynError> {
        std::mem::replace(&mut self.arguments, Ok(None))
    }

    fn persistent(&self) -> bool {
        self.persistent
    }
}

macro_rules! query_span {
    ($name:literal, $sql:expr, $attributes:expr) => {
        tracing::info_span!(
            $name,
            otel.kind = "client",
            db.system.name = "postgresql",
            db.namespace = $attributes.database.as_deref(),
            db.name = $attributes.database.as_deref(),
            db.query.text = $sql.as_str(),
            db.response.affected_rows = field::Empty,
            db.response.returned_rows = field::Empty,
            db.response.status_code = field::Empty,
            server.address = $attributes.host.as_str(),
            server.port = i64::from($attributes.port),
            net.peer.name = $attributes.host.as_str(),
            net.peer.port = i64::from($attributes.port),
            peer.service = "toki-api-db",
            error.type = field::Empty,
            error.message = field::Empty,
            otel.status_code = field::Empty,
            otel.status_description = field::Empty,
        )
    };
}

fn record_error(span: &Span, error: &Error) {
    // PostgreSQL error messages may quote bound data (for example a failed
    // cast). Export classifications and SQLSTATE, never the raw error payload.
    let message = match error {
        Error::Database(_) => "Database query failed",
        Error::RowNotFound => "Query returned no rows",
        Error::Encode(_) => "Query argument encoding failed",
        Error::Decode(_) | Error::ColumnDecode { .. } => "Query result decoding failed",
        Error::PoolTimedOut => "Database connection pool timed out",
        Error::PoolClosed => "Database connection pool closed",
        _ => "Database operation failed",
    };
    span.record("otel.status_code", "error");
    span.record("otel.status_description", message);
    span.record("error.message", message);
    span.record(
        "error.type",
        match error {
            Error::ColumnIndexOutOfBounds { .. }
            | Error::ColumnDecode { .. }
            | Error::ColumnNotFound(_)
            | Error::Decode { .. }
            | Error::Encode { .. }
            | Error::RowNotFound
            | Error::TypeNotFound { .. } => "client",
            _ => "server",
        },
    );
    if let Some(code) = error.as_database_error().and_then(|error| error.code()) {
        span.record("db.response.status_code", code.as_ref());
    }
}

fn trace_result<T>(result: Result<T, Error>) -> Result<T, Error> {
    result.inspect_err(|error| record_error(&Span::current(), error))
}

fn trace_stream<'e>(
    mut stream: BoxStream<'e, Result<Either<PgQueryResult, PgRow>, Error>>,
    span: Span,
) -> BoxStream<'e, Result<Either<PgQueryResult, PgRow>, Error>> {
    let mut trace = StreamTrace {
        span,
        returned: 0,
        affected: 0,
    };
    Box::pin(futures::stream::poll_fn(move |context| {
        trace.poll(&mut stream, context)
    }))
}

struct StreamTrace {
    span: Span,
    returned: u64,
    affected: u64,
}

impl StreamTrace {
    // Enter the span while polling, including database I/O. Never hold a span
    // guard across a suspension, which could attach unrelated work to it.
    fn poll(
        &mut self,
        stream: &mut BoxStream<'_, Result<Either<PgQueryResult, PgRow>, Error>>,
        context: &mut Context<'_>,
    ) -> Poll<Option<Result<Either<PgQueryResult, PgRow>, Error>>> {
        let _entered = self.span.enter();
        let next = stream.as_mut().poll_next(context);
        if let Poll::Ready(Some(result)) = &next {
            match result {
                Ok(Either::Left(result)) => self.affected += result.rows_affected(),
                Ok(Either::Right(_)) => self.returned += 1,
                Err(error) => record_error(&self.span, error),
            }
        }
        next
    }
}

impl Drop for StreamTrace {
    fn drop(&mut self) {
        // Record totals once, including when a caller cancels a stream. OTEL
        // lazy span builders otherwise retain duplicate incremental fields.
        record_rows(&self.span, "db.response.affected_rows", self.affected);
        record_rows(&self.span, "db.response.returned_rows", self.returned);
    }
}

fn record_rows(span: &Span, field: &'static str, count: u64) {
    // OpenTelemetry's integer attributes are signed; its tracing visitor
    // records unsigned values as strings.
    span.record(field, i64::try_from(count).unwrap_or(i64::MAX));
}

// Pools and transaction connections share the same tracing behavior. SQLx's
// default fetch/execute_many methods use fetch_many and retain this span.
macro_rules! traced_executor {
    ($this:ident, $inner:expr, $attributes:expr) => {
        type Database = Postgres;

        fn execute<'e, 'q: 'e, E>(
            $this,
            query: E,
        ) -> BoxFuture<'e, Result<PgQueryResult, Error>>
        where
            'c: 'e,
            E: 'q + Execute<'q, Postgres>,
        {
            let query = Query::new(query);
            let span = query_span!("sqlx.execute", query.sql, $attributes);
            let future = ($inner).execute(query);
            Box::pin(async move {
                trace_result(future.await).inspect(|result| {
                    record_rows(&Span::current(), "db.response.affected_rows", result.rows_affected());
                })
            }.instrument(span))
        }

        fn fetch_many<'e, 'q: 'e, E>(
            $this,
            query: E,
        ) -> BoxStream<'e, Result<Either<PgQueryResult, PgRow>, Error>>
        where
            'c: 'e,
            E: 'q + Execute<'q, Postgres>,
        {
            let query = Query::new(query);
            let span = query_span!("sqlx.fetch_many", query.sql, $attributes);
            trace_stream(($inner).fetch_many(query), span)
        }

        fn fetch_all<'e, 'q: 'e, E>(
            $this,
            query: E,
        ) -> BoxFuture<'e, Result<Vec<PgRow>, Error>>
        where
            'c: 'e,
            E: 'q + Execute<'q, Postgres>,
        {
            let query = Query::new(query);
            let span = query_span!("sqlx.fetch_all", query.sql, $attributes);
            let future = ($inner).fetch_all(query);
            Box::pin(async move {
                trace_result(future.await).inspect(|rows| {
                    record_rows(&Span::current(), "db.response.returned_rows", rows.len() as u64);
                })
            }.instrument(span))
        }

        fn fetch_one<'e, 'q: 'e, E>(
            $this,
            query: E,
        ) -> BoxFuture<'e, Result<PgRow, Error>>
        where
            'c: 'e,
            E: 'q + Execute<'q, Postgres>,
        {
            let query = Query::new(query);
            let span = query_span!("sqlx.fetch_one", query.sql, $attributes);
            let future = ($inner).fetch_one(query);
            Box::pin(async move {
                trace_result(future.await).inspect(|_| {
                    record_rows(&Span::current(), "db.response.returned_rows", 1);
                })
            }.instrument(span))
        }

        fn fetch_optional<'e, 'q: 'e, E>(
            $this,
            query: E,
        ) -> BoxFuture<'e, Result<Option<PgRow>, Error>>
        where
            'c: 'e,
            E: 'q + Execute<'q, Postgres>,
        {
            let query = Query::new(query);
            let span = query_span!("sqlx.fetch_optional", query.sql, $attributes);
            let future = ($inner).fetch_optional(query);
            Box::pin(async move {
                trace_result(future.await).inspect(|row| {
                    record_rows(&Span::current(), "db.response.returned_rows", u64::from(row.is_some()));
                })
            }.instrument(span))
        }

        fn prepare_with<'e>(
            $this,
            sql: SqlStr,
            parameters: &'e [PgTypeInfo],
        ) -> BoxFuture<'e, Result<PgStatement, Error>>
        where
            'c: 'e,
        {
            let span = query_span!("sqlx.prepare_with", sql, $attributes);
            let future = ($inner).prepare_with(sql, parameters);
            Box::pin(async move { trace_result(future.await) }.instrument(span))
        }

        fn describe<'e>(
            $this,
            sql: SqlStr,
        ) -> BoxFuture<'e, Result<sqlx::Describe<Postgres>, Error>>
        where
            'c: 'e,
        {
            let span = query_span!("sqlx.describe", sql, $attributes);
            let future = ($inner).describe(sql);
            Box::pin(async move { trace_result(future.await) }.instrument(span))
        }
    };
}

impl<'c> Executor<'c> for &'c DbPool {
    traced_executor!(self, &self.inner, &self.attributes);
}

impl<'c> Executor<'c> for &'c mut DbConnection<'_> {
    traced_executor!(self, &mut *self.inner, self.attributes);
}

#[cfg(test)]
#[path = "db_tests.rs"]
mod tests;

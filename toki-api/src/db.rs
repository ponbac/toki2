use crate::config::DatabaseSettings;

pub(crate) type DbPool = sqlx_tracing::Pool<sqlx::Postgres>;

pub(crate) fn traced_pool(pool: sqlx::PgPool, settings: &DatabaseSettings) -> DbPool {
    sqlx_tracing::PoolBuilder::from(pool)
        .with_name("toki-api-db")
        .with_database(settings.database_name.clone())
        .with_host(settings.host.clone())
        .with_port(settings.port)
        .build()
}

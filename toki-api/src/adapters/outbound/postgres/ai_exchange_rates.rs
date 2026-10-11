use async_trait::async_trait;

use crate::{
    db::DbPool,
    domain::{
        models::{
            AiBillingMonth, AiCurrency, AiExchangeRate, AiExchangeRateOverride,
            AiExchangeRateValue, AiFetchedExchangeRate, AiRateClock, AiUsageTimeZone, UserId,
        },
        ports::outbound::{AiExchangeRateRepository, AiFetchedRateWrite},
        AiBillingError,
    },
};

/// Exchange rates in `ai_billing_exchange_rates`, exact to a millionth. Each
/// row keeps the fetched rate and an admin's override apart.
pub struct PostgresAiExchangeRateRepository {
    pool: DbPool,
}

impl PostgresAiExchangeRateRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

fn rate_value(
    millionths: i64,
    currency: &AiCurrency,
) -> Result<AiExchangeRateValue, AiBillingError> {
    AiExchangeRateValue::from_millionths(millionths).ok_or_else(|| {
        AiBillingError::Storage(format!("invalid stored {} rate", currency.as_str()))
    })
}

#[async_trait]
impl AiExchangeRateRepository for PostgresAiExchangeRateRepository {
    async fn month_rates(
        &self,
        month: AiBillingMonth,
        time_zone: &AiUsageTimeZone,
    ) -> Result<Vec<AiExchangeRate>, AiBillingError> {
        let rows = sqlx::query!(
            r#"
            SELECT
                rate.currency,
                round(rate.fetched_rate * 1000000)::int8 AS "fetched_millionths?",
                rate.fetched_source,
                rate.fetched_provisional,
                rate.fetched_through,
                rate.fetched_at,
                (rate.fetched_at AT TIME ZONE $2)::date AS "fetched_on?",
                round(rate.override_rate * 1000000)::int8 AS "override_millionths?",
                (rate.overridden_at AT TIME ZONE $2)::date AS "overridden_on?",
                editor.full_name AS "overridden_by?"
            FROM ai_billing_exchange_rates AS rate
            LEFT JOIN users AS editor ON editor.id = rate.overridden_by
            WHERE rate.month = $1
            ORDER BY rate.currency
            "#,
            month.first_day(),
            time_zone.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        rows.into_iter()
            .map(|row| {
                let currency = AiCurrency::parse(&row.currency).map_err(|_| {
                    AiBillingError::Storage(format!("invalid stored currency {:?}", row.currency))
                })?;
                let fetched = match (
                    row.fetched_millionths,
                    row.fetched_source,
                    row.fetched_provisional,
                    row.fetched_through,
                    row.fetched_at,
                    row.fetched_on,
                ) {
                    (
                        Some(millionths),
                        Some(provider),
                        Some(provisional),
                        Some(observed_through),
                        Some(fetched_at),
                        Some(fetched_on),
                    ) => Some(AiFetchedExchangeRate {
                        rate: rate_value(millionths, &currency)?,
                        provider,
                        provisional,
                        observed_through,
                        fetched_at,
                        fetched_on,
                    }),
                    _ => None,
                };
                let overridden = match (row.override_millionths, row.overridden_on) {
                    (Some(millionths), Some(set_on)) => Some(AiExchangeRateOverride {
                        rate: rate_value(millionths, &currency)?,
                        set_on,
                        by: row.overridden_by,
                    }),
                    _ => None,
                };

                Ok(AiExchangeRate {
                    month,
                    currency,
                    fetched,
                    overridden,
                })
            })
            .collect()
    }

    async fn store_fetched(&self, rate: &AiFetchedRateWrite) -> Result<(), AiBillingError> {
        // The override columns are left alone, and a final rate is never
        // replaced by a provisional one.
        sqlx::query!(
            r#"
            INSERT INTO ai_billing_exchange_rates
                (month, currency, fetched_rate, fetched_source, fetched_provisional,
                 fetched_through, fetched_at)
            VALUES ($1, $2, $3::int8::numeric / 1000000, $4, $5, $6, now())
            ON CONFLICT (month, currency) DO UPDATE
            SET fetched_rate = EXCLUDED.fetched_rate,
                fetched_source = EXCLUDED.fetched_source,
                fetched_provisional = EXCLUDED.fetched_provisional,
                fetched_through = EXCLUDED.fetched_through,
                fetched_at = EXCLUDED.fetched_at
            WHERE ai_billing_exchange_rates.fetched_rate IS NULL
               OR ai_billing_exchange_rates.fetched_provisional
               OR NOT EXCLUDED.fetched_provisional
            "#,
            rate.month.first_day(),
            rate.currency.as_str(),
            rate.provided.rate.millionths(),
            rate.provider,
            rate.provisional,
            rate.provided.observed_through,
        )
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(())
    }

    async fn set_override(
        &self,
        month: AiBillingMonth,
        currency: &AiCurrency,
        rate: AiExchangeRateValue,
        by: &UserId,
    ) -> Result<(), AiBillingError> {
        sqlx::query!(
            r#"
            INSERT INTO ai_billing_exchange_rates
                (month, currency, override_rate, overridden_at, overridden_by)
            VALUES ($1, $2, $3::int8::numeric / 1000000, now(), $4)
            ON CONFLICT (month, currency) DO UPDATE
            SET override_rate = EXCLUDED.override_rate,
                overridden_at = EXCLUDED.overridden_at,
                overridden_by = EXCLUDED.overridden_by
            "#,
            month.first_day(),
            currency.as_str(),
            rate.millionths(),
            by.as_i32(),
        )
        .execute(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(())
    }

    async fn remove_override(
        &self,
        month: AiBillingMonth,
        currency: &AiCurrency,
    ) -> Result<(), AiBillingError> {
        let mut transaction = self.pool.begin().await.map_err(storage_error)?;
        // Decide from the locked row: a background fetch can otherwise turn an
        // override-only row into a fetched row while a conditional DELETE waits,
        // leaving the override untouched.
        let has_fetched = sqlx::query_scalar!(
            r#"
            SELECT fetched_rate IS NOT NULL AS "has_fetched!"
            FROM ai_billing_exchange_rates
            WHERE month = $1 AND currency = $2
            FOR UPDATE
            "#,
            month.first_day(),
            currency.as_str(),
        )
        .fetch_optional(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        if has_fetched == Some(true) {
            sqlx::query!(
                r#"
                UPDATE ai_billing_exchange_rates
                SET override_rate = NULL, overridden_at = NULL, overridden_by = NULL
                WHERE month = $1 AND currency = $2
                "#,
                month.first_day(),
                currency.as_str(),
            )
            .execute(&mut transaction.executor())
            .await
            .map_err(storage_error)?;
        } else if has_fetched == Some(false) {
            // An override-only row holds nothing after a reset.
            sqlx::query!(
                r#"
                DELETE FROM ai_billing_exchange_rates
                WHERE month = $1 AND currency = $2
                "#,
                month.first_day(),
                currency.as_str(),
            )
            .execute(&mut transaction.executor())
            .await
            .map_err(storage_error)?;
        }
        transaction.commit().await.map_err(storage_error)?;
        Ok(())
    }

    async fn clock(&self, time_zone: &AiUsageTimeZone) -> Result<AiRateClock, AiBillingError> {
        let row = sqlx::query!(
            r#"SELECT now() AS "now!", (now() AT TIME ZONE $1)::date AS "today!""#,
            time_zone.as_str(),
        )
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(AiRateClock {
            now: row.now,
            today: row.today,
        })
    }
}

fn storage_error(error: sqlx::Error) -> AiBillingError {
    AiBillingError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use sqlx::PgPool;
    use time::macros::date;

    use super::*;
    use crate::domain::models::AiProvidedRate;

    fn db(pool: &PgPool) -> DbPool {
        sqlx_tracing::PoolBuilder::from(pool.clone()).build()
    }

    fn stockholm() -> AiUsageTimeZone {
        AiUsageTimeZone::parse("Europe/Stockholm").unwrap()
    }

    fn september() -> AiBillingMonth {
        AiBillingMonth::parse("2026-09").unwrap()
    }

    fn usd() -> AiCurrency {
        AiCurrency::usd()
    }

    fn value(raw: &str) -> AiExchangeRateValue {
        AiExchangeRateValue::parse(raw).unwrap()
    }

    fn fetched(raw: &str, provisional: bool) -> AiFetchedRateWrite {
        AiFetchedRateWrite {
            month: september(),
            currency: usd(),
            provided: AiProvidedRate {
                rate: value(raw),
                observed_through: date!(2026 - 09 - 29),
            },
            provider: "riksbank",
            provisional,
        }
    }

    async fn admin(pool: &PgPool) -> UserId {
        let id: i32 = sqlx::query_scalar(
            "INSERT INTO users (email, full_name, picture, access_token, roles)
             VALUES ('admin@example.com', 'Admin Example', '', '', '{User,Admin}')
             RETURNING id",
        )
        .fetch_one(pool)
        .await
        .unwrap();
        UserId::new(id)
    }

    /// `(rate billed at, source, provisional, fetched rate)` per currency.
    async fn stored(
        repository: &PostgresAiExchangeRateRepository,
    ) -> Vec<(String, String, bool, Option<String>)> {
        repository
            .month_rates(september(), &stockholm())
            .await
            .unwrap()
            .into_iter()
            .map(|rate| {
                (
                    rate.rate().unwrap().to_decimal(),
                    rate.source().to_string(),
                    rate.is_provisional(),
                    rate.fetched.map(|fetched| fetched.rate.to_decimal()),
                )
            })
            .collect()
    }

    fn row(
        rate: &str,
        source: &str,
        provisional: bool,
        fetched: Option<&str>,
    ) -> (String, String, bool, Option<String>) {
        (
            rate.to_string(),
            source.to_string(),
            provisional,
            fetched.map(str::to_string),
        )
    }

    #[sqlx::test]
    async fn a_final_rate_replaces_a_provisional_one_but_not_the_reverse(pool: PgPool) {
        let repository = PostgresAiExchangeRateRepository::new(db(&pool));
        repository
            .store_fetched(&fetched("9.1", true))
            .await
            .unwrap();
        repository
            .store_fetched(&fetched("9.2", true))
            .await
            .unwrap();
        assert_eq!(
            stored(&repository).await,
            [row("9.20", "riksbank", true, Some("9.20"))]
        );
        let rates = repository
            .month_rates(september(), &stockholm())
            .await
            .unwrap();
        assert_eq!(
            rates[0].fetched.as_ref().unwrap().observed_through,
            date!(2026 - 09 - 29)
        );

        repository
            .store_fetched(&fetched("9.508431", false))
            .await
            .unwrap();
        repository
            .store_fetched(&fetched("9.3", true))
            .await
            .unwrap();
        assert_eq!(
            stored(&repository).await,
            [row("9.508431", "riksbank", false, Some("9.508431"))]
        );
    }

    #[sqlx::test]
    async fn the_provider_name_is_stored_as_data(pool: PgPool) {
        let repository = PostgresAiExchangeRateRepository::new(db(&pool));
        repository
            .store_fetched(&AiFetchedRateWrite {
                provider: "another-bank",
                ..fetched("9.1", false)
            })
            .await
            .unwrap();
        assert_eq!(
            stored(&repository).await,
            [row("9.10", "another-bank", false, Some("9.10"))]
        );
    }

    #[sqlx::test]
    async fn an_override_keeps_the_fetched_rate_beneath_it(pool: PgPool) {
        let repository = PostgresAiExchangeRateRepository::new(db(&pool));
        let admin = admin(&pool).await;
        repository
            .store_fetched(&fetched("9.1", true))
            .await
            .unwrap();
        repository
            .set_override(september(), &usd(), value("10.25"), &admin)
            .await
            .unwrap();
        assert_eq!(
            stored(&repository).await,
            [row("10.25", "admin", false, Some("9.10"))]
        );

        // Later fetches update the fetched rate and keep the override.
        repository
            .store_fetched(&fetched("9.3", false))
            .await
            .unwrap();
        let rates = repository
            .month_rates(september(), &stockholm())
            .await
            .unwrap();
        assert_eq!(rates.len(), 1);
        assert_eq!(rates[0].rate().unwrap().to_decimal(), "10.25");
        let overridden = rates[0].overridden.as_ref().unwrap();
        assert_eq!(overridden.by.as_deref(), Some("Admin Example"));
        assert_eq!(
            overridden.set_on,
            repository.clock(&stockholm()).await.unwrap().today
        );
        assert_eq!(rates[0].fetched.as_ref().unwrap().rate.to_decimal(), "9.30");

        // Resetting removes only the override: the fetched rate applies again.
        repository
            .remove_override(september(), &usd())
            .await
            .unwrap();
        assert_eq!(
            stored(&repository).await,
            [row("9.30", "riksbank", false, Some("9.30"))]
        );
    }

    #[sqlx::test]
    async fn an_override_without_a_fetched_rate_is_removed_entirely(pool: PgPool) {
        let repository = PostgresAiExchangeRateRepository::new(db(&pool));
        let admin = admin(&pool).await;
        repository
            .set_override(september(), &usd(), value("10.25"), &admin)
            .await
            .unwrap();
        assert_eq!(
            stored(&repository).await,
            [row("10.25", "admin", false, None)]
        );
        repository
            .remove_override(september(), &usd())
            .await
            .unwrap();
        assert!(stored(&repository).await.is_empty());
        // A later fetch stores a rate again.
        repository
            .store_fetched(&fetched("9.4", false))
            .await
            .unwrap();
        assert_eq!(
            stored(&repository).await,
            [row("9.40", "riksbank", false, Some("9.40"))]
        );
    }

    #[sqlx::test]
    async fn rates_belong_to_their_month(pool: PgPool) {
        let repository = PostgresAiExchangeRateRepository::new(db(&pool));
        repository
            .store_fetched(&AiFetchedRateWrite {
                month: AiBillingMonth::parse("2026-08").unwrap(),
                ..fetched("9.9", false)
            })
            .await
            .unwrap();
        assert!(stored(&repository).await.is_empty());
    }

    async fn wait_for_blocked_writes(pool: &PgPool, count: i64) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let blocked: i64 = sqlx::query_scalar(
                    "SELECT count(*) FROM pg_stat_activity
                     WHERE datname = current_database() AND wait_event_type = 'Lock'",
                )
                .fetch_one(pool)
                .await
                .unwrap();
                if blocked >= count {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("both writes must contend for the same rate row");
    }

    #[sqlx::test]
    async fn a_reset_keeps_a_concurrent_fetch_and_removes_the_override(pool: PgPool) {
        let repository = Arc::new(PostgresAiExchangeRateRepository::new(db(&pool)));
        let admin = admin(&pool).await;
        repository
            .set_override(september(), &usd(), value("10.25"), &admin)
            .await
            .unwrap();

        // Queue the fetch before the reset, while both see an override-only
        // row. The fetch adds its rate as the reset waits for the row lock.
        let mut blocker = pool.begin().await.unwrap();
        sqlx::query("SELECT currency FROM ai_billing_exchange_rates FOR UPDATE")
            .execute(&mut *blocker)
            .await
            .unwrap();
        let writer = repository.clone();
        let fetch = tokio::spawn(async move { writer.store_fetched(&fetched("9.4", false)).await });
        wait_for_blocked_writes(&pool, 1).await;
        let resetting = repository.clone();
        let reset =
            tokio::spawn(async move { resetting.remove_override(september(), &usd()).await });
        wait_for_blocked_writes(&pool, 2).await;
        blocker.commit().await.unwrap();
        fetch.await.unwrap().unwrap();
        reset.await.unwrap().unwrap();

        assert_eq!(
            stored(&repository).await,
            [row("9.40", "riksbank", false, Some("9.40"))]
        );
        // Positive control: an ordinary later override and reset keeps the
        // same fetched rate too.
        repository
            .set_override(september(), &usd(), value("11"), &admin)
            .await
            .unwrap();
        repository
            .remove_override(september(), &usd())
            .await
            .unwrap();
        assert_eq!(
            stored(&repository).await,
            [row("9.40", "riksbank", false, Some("9.40"))]
        );
    }

    #[sqlx::test]
    async fn the_table_refuses_zero_rates_empty_sources_and_other_days(pool: PgPool) {
        for (month, rate, source) in [
            ("2026-09-01", "0", "riksbank"),
            ("2026-09-02", "1", "riksbank"),
            ("2026-09-01", "1", ""),
        ] {
            let result = sqlx::query(
                "INSERT INTO ai_billing_exchange_rates
                     (month, currency, fetched_rate, fetched_source, fetched_provisional,
                      fetched_through, fetched_at)
                 VALUES ($1::date, 'USD', $2::numeric, $3, false, '2026-09-29', now())",
            )
            .bind(month)
            .bind(rate)
            .bind(source)
            .execute(&pool)
            .await;
            assert!(result.is_err(), "{month} {rate} {source:?}");
        }
        // Nor a row without any rate.
        let result = sqlx::query(
            "INSERT INTO ai_billing_exchange_rates (month, currency) VALUES ('2026-09-01', 'USD')",
        )
        .execute(&pool)
        .await;
        assert!(result.is_err());
    }
}

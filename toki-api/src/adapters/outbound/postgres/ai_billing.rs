use std::collections::HashMap;

use async_trait::async_trait;
use time::Date;

use super::ai_usage_reads::{local_today, stored_mappings};
use crate::{
    db::DbPool,
    domain::{
        models::{
            AiBillingDeveloper, AiBillingMachine, AiBillingUsage, AiCoverageInterval,
            AiCoverageStatus, AiDailyUsage, AiDeveloperDayUsage, AiLocalDay, AiMachineId,
            AiMappingResolver, AiPricingStatus, AiProvider, AiProviderReport, AiTokenCounts,
            AiUsageDateRange, AiUsageTimeZone, AiUsdNanos, TimeTrackingCompany, UserId,
        },
        ports::outbound::AiBillingRepository,
        AiBillingError,
    },
};

/// Billing reads of AI usage. Every usage query groups buckets by local day at
/// the finest, so no hour or session ever leaves the database through it.
pub struct PostgresAiBillingRepository {
    pool: DbPool,
}

impl PostgresAiBillingRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Attributes project keys as every read of usage does: only mappings to
    /// `mapping_company`, the configured company, resolve.
    async fn resolver(
        &self,
        mapping_company: Option<&TimeTrackingCompany>,
    ) -> Result<AiMappingResolver, AiBillingError> {
        let mappings = stored_mappings(&self.pool).await.map_err(storage_error)?;
        Ok(AiMappingResolver::new(mappings, mapping_company.cloned()))
    }
}

#[async_trait]
impl AiBillingRepository for PostgresAiBillingRepository {
    async fn daily_usage(
        &self,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
        mapping_company: Option<&TimeTrackingCompany>,
        user_id: Option<&UserId>,
    ) -> Result<Vec<AiDailyUsage>, AiBillingError> {
        // As in `period_totals`: local days by the zone's rules, so daylight
        // saving changes and month edges fall where they should. Costs are
        // summed exactly as NUMERIC per resolved project, then rounded to
        // billionths of a dollar. The shared resolver alone decides which
        // mappings resolve; SQL groups its explicit key-to-project projection
        // before rounding, so splitting usage across keys cannot change a bill.
        // The range uses `ai_usage_buckets_hour_start_idx`, or
        // the `(user_id, hour_start)` index for one user.
        let resolver = self.resolver(mapping_company).await?;
        let resolved = resolver.resolved_keys();
        let keys: Vec<&str> = resolved.iter().map(|(key, _)| *key).collect();
        let ids: Vec<&str> = resolved
            .iter()
            .map(|(_, project)| project.id.as_str())
            .collect();
        let rows = sqlx::query!(
            r#"
            WITH resolved_projects AS (
                SELECT project_key, project_id
                FROM UNNEST($5::text[], $6::text[]) AS mapping(project_key, project_id)
            )
            SELECT
                bucket.user_id,
                bucket.provider,
                (bucket.hour_start AT TIME ZONE $3)::date AS "day!",
                MIN(bucket.project_key) AS "project_key!",
                SUM(bucket.input_tokens)::int8 AS "input_tokens!",
                SUM(bucket.cache_read_tokens)::int8 AS "cache_read_tokens!",
                SUM(bucket.cache_write_tokens)::int8 AS "cache_write_tokens!",
                SUM(bucket.output_tokens)::int8 AS "output_tokens!",
                SUM(bucket.records)::int8 AS "records!",
                round(COALESCE(SUM(bucket.estimated_cost_usd), 0) * 1000000000)::int8
                    AS "priced_cost_nanos!",
                SUM(bucket.unpriced_records)::int8 AS "unpriced_records!"
            FROM ai_usage_buckets AS bucket
            LEFT JOIN resolved_projects AS mapping ON mapping.project_key = bucket.project_key
            WHERE bucket.hour_start >= ($1::date::timestamp AT TIME ZONE $3)
              AND bucket.hour_start < ($2::date::timestamp AT TIME ZONE $3)
              AND ($4::int4 IS NULL OR bucket.user_id = $4)
            GROUP BY bucket.user_id, bucket.provider, 3, mapping.project_id
            ORDER BY bucket.user_id, bucket.provider, 3, mapping.project_id NULLS LAST
            "#,
            dates.start(),
            dates.end(),
            time_zone.as_str(),
            user_id.map(UserId::as_i32),
            &keys as &[&str],
            &ids as &[&str],
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        rows.into_iter()
            .map(|row| {
                Ok(AiDailyUsage {
                    user_id: UserId::new(row.user_id),
                    provider: provider(&row.provider)?,
                    day: row.day,
                    // Mappings may hold different names for a renamed project.
                    // Preserve the first used key's name, as before grouping
                    // in SQL; every key in the group resolves to the same id.
                    project: resolver.project(&row.project_key).cloned(),
                    usage: usage(
                        [
                            row.input_tokens,
                            row.cache_read_tokens,
                            row.cache_write_tokens,
                            row.output_tokens,
                        ],
                        row.records,
                        row.priced_cost_nanos,
                        row.unpriced_records,
                    )?,
                })
            })
            .collect()
    }

    async fn developer_usage(
        &self,
        user_id: &UserId,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
        mapping_company: Option<&TimeTrackingCompany>,
    ) -> Result<Vec<AiDeveloperDayUsage>, AiBillingError> {
        let rows = sqlx::query!(
            r#"
            SELECT
                (bucket.hour_start AT TIME ZONE $4)::date AS "day!",
                bucket.provider,
                bucket.model,
                bucket.machine_id::text AS "machine_id!",
                bucket.project_key,
                SUM(bucket.input_tokens)::int8 AS "input_tokens!",
                SUM(bucket.cache_read_tokens)::int8 AS "cache_read_tokens!",
                SUM(bucket.cache_write_tokens)::int8 AS "cache_write_tokens!",
                SUM(bucket.output_tokens)::int8 AS "output_tokens!",
                SUM(bucket.records)::int8 AS "records!",
                round(COALESCE(SUM(bucket.estimated_cost_usd), 0) * 1000000000)::int8
                    AS "priced_cost_nanos!",
                SUM(bucket.unpriced_records)::int8 AS "unpriced_records!"
            FROM ai_usage_buckets AS bucket
            WHERE bucket.user_id = $1
              AND bucket.hour_start >= ($2::date::timestamp AT TIME ZONE $4)
              AND bucket.hour_start < ($3::date::timestamp AT TIME ZONE $4)
            GROUP BY 1, bucket.provider, bucket.model, bucket.machine_id, bucket.project_key
            ORDER BY 1, bucket.provider, bucket.model, bucket.machine_id, bucket.project_key
            "#,
            user_id.as_i32(),
            dates.start(),
            dates.end(),
            time_zone.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;
        let resolver = self.resolver(mapping_company).await?;

        rows.into_iter()
            .map(|row| {
                Ok(AiDeveloperDayUsage {
                    day: row.day,
                    provider: provider(&row.provider)?,
                    model: row.model,
                    machine_id: machine_id(&row.machine_id)?,
                    project: resolver.project(&row.project_key).cloned(),
                    project_key: row.project_key,
                    usage: usage(
                        [
                            row.input_tokens,
                            row.cache_read_tokens,
                            row.cache_write_tokens,
                            row.output_tokens,
                        ],
                        row.records,
                        row.priced_cost_nanos,
                        row.unpriced_records,
                    )?,
                })
            })
            .collect()
    }

    async fn local_days(
        &self,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
    ) -> Result<Vec<AiLocalDay>, AiBillingError> {
        let rows = sqlx::query!(
            r#"
            SELECT
                ($1::date + offset_days) AS "day!",
                (($1::date + offset_days)::timestamp AT TIME ZONE $3) AS "start!",
                (($1::date + offset_days + 1)::timestamp AT TIME ZONE $3) AS "end!"
            FROM generate_series(0, $2::date - $1::date - 1) AS offset_days
            ORDER BY 1
            "#,
            dates.start(),
            dates.end(),
            time_zone.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        Ok(rows
            .into_iter()
            .map(|row| AiLocalDay {
                day: row.day,
                start: row.start,
                end: row.end,
            })
            .collect())
    }

    async fn machines(
        &self,
        user_id: Option<&UserId>,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
    ) -> Result<Vec<AiBillingMachine>, AiBillingError> {
        let user_id = user_id.map(UserId::as_i32);
        let machines = sqlx::query!(
            r#"
            SELECT
                machine.id::text AS "id!",
                machine.user_id,
                machine.label,
                machine.client_version,
                machine.last_synced_at,
                (machine.last_synced_at AT TIME ZONE $3)::date AS "last_synced_on!",
                machine.last_synced_at >= ($2::date::timestamp AT TIME ZONE $3)
                    AS "synced_since_month_start!"
            FROM ai_usage_machines AS machine
            WHERE $1::int4 IS NULL OR machine.user_id = $1
            ORDER BY machine.user_id, machine.label, machine.id
            "#,
            user_id,
            dates.start(),
            time_zone.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        let used = sqlx::query!(
            r#"
            SELECT bucket.machine_id::text AS "machine_id!", bucket.provider
            FROM ai_usage_buckets AS bucket
            WHERE bucket.hour_start >= ($2::date::timestamp AT TIME ZONE $4)
              AND bucket.hour_start < ($3::date::timestamp AT TIME ZONE $4)
              AND ($1::int4 IS NULL OR bucket.user_id = $1)
            GROUP BY bucket.machine_id, bucket.provider
            "#,
            user_id,
            dates.start(),
            dates.end(),
            time_zone.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        let reports = sqlx::query!(
            r#"
            SELECT
                coverage.machine_id::text AS "machine_id!",
                coverage.provider,
                coverage.status,
                coverage.pricing_status
            FROM ai_usage_machine_coverage AS coverage
            JOIN ai_usage_machines AS machine ON machine.id = coverage.machine_id
            WHERE $1::int4 IS NULL OR machine.user_id = $1
            "#,
            user_id,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        let intervals = sqlx::query!(
            r#"
            SELECT
                log.machine_id::text AS "machine_id!",
                log.provider,
                log.window_start,
                log.covered_until
            FROM ai_usage_coverage_log AS log
            JOIN ai_usage_machines AS machine ON machine.id = log.machine_id
            WHERE ($1::int4 IS NULL OR machine.user_id = $1)
              AND log.covered_until > ($2::date::timestamp AT TIME ZONE $4)
              AND log.window_start < ($3::date::timestamp AT TIME ZONE $4)
            "#,
            user_id,
            dates.start(),
            dates.end(),
            time_zone.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(storage_error)?;

        let mut used_by: HashMap<String, Vec<AiProvider>> = HashMap::new();
        for row in used {
            used_by
                .entry(row.machine_id)
                .or_default()
                .push(provider(&row.provider)?);
        }
        let mut reports_by: HashMap<String, Vec<AiProviderReport>> = HashMap::new();
        for row in reports {
            reports_by
                .entry(row.machine_id)
                .or_default()
                .push(AiProviderReport {
                    provider: provider(&row.provider)?,
                    status: coverage_status(&row.status)?,
                    pricing_status: row
                        .pricing_status
                        .as_deref()
                        .map(pricing_status)
                        .transpose()?,
                });
        }
        let mut intervals_by: HashMap<String, Vec<AiCoverageInterval>> = HashMap::new();
        for row in intervals {
            intervals_by
                .entry(row.machine_id)
                .or_default()
                .push(AiCoverageInterval {
                    provider: provider(&row.provider)?,
                    start: row.window_start,
                    end: row.covered_until,
                });
        }

        machines
            .into_iter()
            .map(|row| {
                Ok(AiBillingMachine {
                    user_id: UserId::new(row.user_id),
                    machine_id: machine_id(&row.id)?,
                    providers_with_usage: used_by.remove(&row.id).unwrap_or_default(),
                    latest_reports: reports_by.remove(&row.id).unwrap_or_default(),
                    intervals: intervals_by.remove(&row.id).unwrap_or_default(),
                    label: row.label,
                    client_version: row.client_version,
                    last_synced_at: row.last_synced_at,
                    last_synced_on: row.last_synced_on,
                    synced_since_month_start: row.synced_since_month_start,
                })
            })
            .collect()
    }

    async fn users(&self) -> Result<Vec<AiBillingDeveloper>, AiBillingError> {
        let rows = sqlx::query!("SELECT id, full_name, email FROM users ORDER BY full_name, id")
            .fetch_all(&self.pool)
            .await
            .map_err(storage_error)?;

        Ok(rows
            .into_iter()
            .map(|row| AiBillingDeveloper {
                user_id: UserId::new(row.id),
                full_name: row.full_name,
                email: row.email,
            })
            .collect())
    }

    async fn today(&self, time_zone: &AiUsageTimeZone) -> Result<Date, AiBillingError> {
        local_today(&self.pool, time_zone)
            .await
            .map_err(storage_error)
    }
}

fn usage(
    [input, cache_read, cache_write, output]: [i64; 4],
    records: i64,
    priced_cost_nanos: i64,
    unpriced_records: i64,
) -> Result<AiBillingUsage, AiBillingError> {
    let usage = AiBillingUsage {
        tokens: AiTokenCounts {
            input,
            cache_read,
            cache_write,
            output,
        },
        records,
        priced_cost: AiUsdNanos::new(priced_cost_nanos)
            .ok_or_else(|| corrupt("a summed cost is negative"))?,
        unpriced_records,
    };
    usage.ensure_reportable()?;
    Ok(usage)
}

fn provider(raw: &str) -> Result<AiProvider, AiBillingError> {
    AiProvider::parse(raw).ok_or_else(|| corrupt(format!("unknown provider {raw:?}")))
}

fn machine_id(raw: &str) -> Result<AiMachineId, AiBillingError> {
    AiMachineId::parse(raw).ok_or_else(|| corrupt(format!("invalid machine id {raw:?}")))
}

fn coverage_status(raw: &str) -> Result<AiCoverageStatus, AiBillingError> {
    AiCoverageStatus::parse(raw).ok_or_else(|| corrupt(format!("unknown coverage status {raw:?}")))
}

fn pricing_status(raw: &str) -> Result<AiPricingStatus, AiBillingError> {
    AiPricingStatus::parse(raw).ok_or_else(|| corrupt(format!("unknown pricing status {raw:?}")))
}

fn corrupt(message: impl Into<String>) -> AiBillingError {
    AiBillingError::Storage(message.into())
}

fn storage_error(error: sqlx::Error) -> AiBillingError {
    if error
        .as_database_error()
        .and_then(|error| error.code())
        .as_deref()
        == Some("22003")
    {
        return AiBillingError::NumericRange;
    }
    AiBillingError::Storage(error.to_string())
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;
    use time::{
        macros::{date, datetime},
        OffsetDateTime,
    };

    use crate::{
        adapters::outbound::postgres::{
            PostgresAiProjectMappingRepository, PostgresAiUsageRepository,
        },
        domain::{
            models::{
                AiBillingMonth, AiMachine, AiMappedProject, AiPricing, AiProjectKey,
                AiProviderCoverage, AiUsageBucket, AiUsageUpload, AiUsageWindow, ProjectId,
                UNATTRIBUTED_PROJECT_KEY,
            },
            ports::outbound::{AiProjectMappingRepository, AiUsageRepository},
        },
    };

    use super::*;

    const MACHINE: &str = "5f0c5a1e-3b8e-4d8e-9a57-0d7b1c1f2e3a";
    const MAPPED: &str = "github.com/example/app";
    const STALE: &str = "github.com/example/legacy";

    fn db(pool: &PgPool) -> DbPool {
        sqlx_tracing::PoolBuilder::from(pool.clone()).build()
    }

    fn company(company_id: &str) -> TimeTrackingCompany {
        TimeTrackingCompany {
            provider: "kleer".to_string(),
            company_id: company_id.to_string(),
        }
    }

    fn bucket(
        hour_start: OffsetDateTime,
        project_key: &str,
        records: i64,
        cost: f64,
    ) -> AiUsageBucket {
        AiUsageBucket {
            hour_start,
            session_key: format!("{records:032x}"),
            project_key: project_key.to_string(),
            provider: AiProvider::Claude,
            model: "example-model".to_string(),
            tokens: AiTokenCounts {
                input: records,
                ..AiTokenCounts::default()
            },
            records,
            estimated_cost_usd: Some(cost),
            unpriced_records: 0,
        }
    }

    async fn setup(
        pool: &PgPool,
        window: (OffsetDateTime, OffsetDateTime),
        buckets: Vec<AiUsageBucket>,
    ) -> UserId {
        let user = UserId::new(
            sqlx::query_scalar(
                "INSERT INTO users (email, full_name, picture, access_token)
                 VALUES ('dev@example.com', 'Dev Example', '', '')
                 RETURNING id",
            )
            .fetch_one(pool)
            .await
            .unwrap(),
        );
        let upload = AiUsageUpload::new(
            AiMachine {
                id: AiMachineId::parse(MACHINE).unwrap(),
                label: "laptop".to_string(),
                client_version: "0.2.0".to_string(),
                time_zone: "Europe/Stockholm".to_string(),
            },
            AiUsageWindow::new(window.0, window.1).unwrap(),
            AiPricing {
                status: AiPricingStatus::Cached,
                fetched_at: None,
                source: "https://example.com/prices.json".to_string(),
            },
            buckets,
            vec![AiProviderCoverage {
                provider: AiProvider::Claude,
                status: AiCoverageStatus::Partial,
                files: 1,
                unreadable: 0,
                malformed_lines: 2,
                skipped_records: 0,
                duplicates: 0,
            }],
            Vec::new(),
        )
        .unwrap();
        PostgresAiUsageRepository::new(db(pool))
            .replace_window(&user, &upload)
            .await
            .unwrap();
        user
    }

    #[sqlx::test]
    async fn splitting_usage_across_keys_of_one_project_does_not_change_the_bill(pool: PgPool) {
        let user = setup(
            &pool,
            (
                datetime!(2026-09-01 00:00 UTC),
                datetime!(2026-09-02 00:00 UTC),
            ),
            vec![
                bucket(datetime!(2026-09-01 00:00 UTC), MAPPED, 1, 0.0024999996),
                bucket(datetime!(2026-09-01 01:00 UTC), MAPPED, 1, 0.0024999996),
            ],
        )
        .await;
        let mapping_repository = PostgresAiProjectMappingRepository::new(db(&pool));
        let project = AiMappedProject {
            id: ProjectId::new("101"),
            name: "Client A".to_string(),
        };
        let configured = company("company-1");
        for key in [MAPPED, STALE] {
            mapping_repository
                .upsert(
                    &AiProjectKey::parse(key).unwrap(),
                    &configured,
                    &project,
                    &user,
                )
                .await
                .unwrap();
        }
        let repository = PostgresAiBillingRepository::new(db(&pool));
        let month = AiBillingMonth::parse("2026-09").unwrap();
        let zone = AiUsageTimeZone::parse("Europe/Stockholm").unwrap();
        let one_key = repository
            .daily_usage(month.dates(), &zone, Some(&configured), Some(&user))
            .await
            .unwrap();
        assert_eq!(one_key.len(), 1);
        assert_eq!(one_key[0].usage.priced_cost.nanos(), 4_999_999);

        sqlx::query("UPDATE ai_usage_buckets SET project_key = $1 WHERE hour_start = $2")
            .bind(STALE)
            .bind(datetime!(2026-09-01 01:00 UTC))
            .execute(&pool)
            .await
            .unwrap();
        let two_keys = repository
            .daily_usage(month.dates(), &zone, Some(&configured), Some(&user))
            .await
            .unwrap();
        assert_eq!(two_keys, one_key);
        let bill = crate::domain::models::bill_month(month, &two_keys, &[]).unwrap();
        assert_eq!(bill.lines[0].billable().unwrap().to_decimal(), "0.00");

        let unconfigured = repository
            .daily_usage(month.dates(), &zone, None, Some(&user))
            .await
            .unwrap();
        assert_eq!(unconfigured.len(), 1);
        assert_eq!(unconfigured[0].project, None);
        assert_eq!(unconfigured[0].usage, one_key[0].usage);
    }

    #[sqlx::test]
    async fn daily_usage_follows_stockholm_days_across_daylight_saving_and_month_edges(
        pool: PgPool,
    ) {
        // Stockholm is UTC+1 until 29 March 2026 at 01:00 UTC, then UTC+2 until
        // 25 October at 01:00 UTC.
        let user = setup(
            &pool,
            (
                datetime!(2026-02-28 00:00 UTC),
                datetime!(2026-11-01 00:00 UTC),
            ),
            vec![
                // Midnight on 1 March, still February in UTC.
                bucket(datetime!(2026-02-28 23:00 UTC), MAPPED, 1, 0.1),
                // 23:00 on 28 March and midnight on 29 March, standard time.
                bucket(datetime!(2026-03-28 22:00 UTC), MAPPED, 2, 0.2),
                bucket(datetime!(2026-03-28 23:00 UTC), MAPPED, 4, 0.1),
                // 23:00 on 29 March, summer time: still the 23-hour day.
                bucket(datetime!(2026-03-29 21:00 UTC), MAPPED, 8, 0.2),
                bucket(datetime!(2026-03-29 22:00 UTC), STALE, 16, 0.3),
                bucket(
                    datetime!(2026-03-31 21:00 UTC),
                    UNATTRIBUTED_PROJECT_KEY,
                    32,
                    0.4,
                ),
                // Midnight on 1 April, summer time: not March.
                bucket(datetime!(2026-03-31 22:00 UTC), MAPPED, 64, 0.5),
                // The 25-hour day of 25 October: 00:00 summer time to 23:00 standard.
                bucket(datetime!(2026-10-24 22:00 UTC), MAPPED, 128, 0.1),
                bucket(datetime!(2026-10-25 22:00 UTC), MAPPED, 256, 0.1),
                bucket(datetime!(2026-10-25 23:00 UTC), MAPPED, 512, 0.1),
            ],
        )
        .await;
        let mappings = PostgresAiProjectMappingRepository::new(db(&pool));
        let project = AiMappedProject {
            id: ProjectId::new("101"),
            name: "Client A".to_string(),
        };
        mappings
            .upsert(
                &AiProjectKey::parse(MAPPED).unwrap(),
                &company("company-1"),
                &project,
                &user,
            )
            .await
            .unwrap();
        // A mapping to another company's project is stale: Unassigned.
        mappings
            .upsert(
                &AiProjectKey::parse(STALE).unwrap(),
                &company("company-2"),
                &project,
                &user,
            )
            .await
            .unwrap();
        let repository = PostgresAiBillingRepository::new(db(&pool));
        let stockholm = AiUsageTimeZone::parse("Europe/Stockholm").unwrap();
        let days = |month: &str| {
            let repository = &repository;
            let stockholm = &stockholm;
            let dates = AiBillingMonth::parse(month).unwrap().dates();
            async move {
                repository
                    .daily_usage(dates, stockholm, Some(&company("company-1")), None)
                    .await
                    .unwrap()
                    .into_iter()
                    .map(|row| {
                        assert_eq!((row.user_id, row.provider), (user, AiProvider::Claude));
                        (
                            row.day,
                            row.project.map(|project| project.name),
                            row.usage.records,
                            row.usage.priced_cost.nanos(),
                        )
                    })
                    .collect::<Vec<_>>()
            }
        };

        let client_a = || Some("Client A".to_string());
        assert_eq!(
            days("2026-03").await,
            [
                (date!(2026 - 03 - 01), client_a(), 1, 100_000_000),
                (date!(2026 - 03 - 28), client_a(), 2, 200_000_000),
                // 0.1 + 0.2 is exactly 0.3: costs are summed as NUMERIC.
                (date!(2026 - 03 - 29), client_a(), 12, 300_000_000),
                (date!(2026 - 03 - 30), None, 16, 300_000_000),
                (date!(2026 - 03 - 31), None, 32, 400_000_000),
            ]
        );
        // Without time tracking configured no mapping resolves: all Unassigned.
        let unconfigured = repository
            .daily_usage(
                AiBillingMonth::parse("2026-03").unwrap().dates(),
                &stockholm,
                None,
                None,
            )
            .await
            .unwrap();
        assert_eq!(unconfigured.len(), 5, "{unconfigured:?}");
        assert!(unconfigured.iter().all(|day| day.project.is_none()));
        assert_eq!(
            days("2026-10").await,
            [
                (date!(2026 - 10 - 25), client_a(), 384, 200_000_000),
                (date!(2026 - 10 - 26), client_a(), 512, 100_000_000),
            ]
        );

        let developer_rows = repository
            .developer_usage(
                &user,
                AiBillingMonth::parse("2026-03").unwrap().dates(),
                &stockholm,
                Some(&company("company-1")),
            )
            .await
            .unwrap();
        let keys: Vec<(Date, &str, Option<&str>)> = developer_rows
            .iter()
            .map(|row| {
                (
                    row.day,
                    row.project_key.as_str(),
                    row.project.as_ref().map(|project| project.name.as_str()),
                )
            })
            .collect();
        assert_eq!(
            keys,
            [
                (date!(2026 - 03 - 01), MAPPED, Some("Client A")),
                (date!(2026 - 03 - 28), MAPPED, Some("Client A")),
                (date!(2026 - 03 - 29), MAPPED, Some("Client A")),
                (date!(2026 - 03 - 30), STALE, None),
                (date!(2026 - 03 - 31), UNATTRIBUTED_PROJECT_KEY, None),
            ]
        );
        assert!(developer_rows
            .iter()
            .all(|row| row.machine_id.to_string() == MACHINE && row.model == "example-model"));
    }

    #[sqlx::test]
    async fn daily_usage_can_be_limited_to_one_user(pool: PgPool) {
        let user = setup(
            &pool,
            (
                datetime!(2026-08-09 22:00 UTC),
                datetime!(2026-08-20 22:00 UTC),
            ),
            vec![bucket(datetime!(2026-08-12 08:00 UTC), MAPPED, 1, 0.1)],
        )
        .await;
        let repository = PostgresAiBillingRepository::new(db(&pool));
        let stockholm = AiUsageTimeZone::parse("Europe/Stockholm").unwrap();
        let august = AiBillingMonth::parse("2026-08").unwrap().dates();
        let records = |user_id| {
            let repository = &repository;
            let stockholm = &stockholm;
            async move {
                repository
                    .daily_usage(august, stockholm, None, user_id)
                    .await
                    .unwrap()
                    .iter()
                    .map(|row| row.usage.records)
                    .sum::<i64>()
            }
        };

        assert_eq!(records(None).await, 1);
        assert_eq!(records(Some(&user)).await, 1);
        assert_eq!(records(Some(&UserId::new(user.as_i32() + 1))).await, 0);
    }

    #[sqlx::test]
    async fn local_days_span_23_or_25_hours_on_daylight_saving_changes(pool: PgPool) {
        let repository = PostgresAiBillingRepository::new(db(&pool));
        let stockholm = AiUsageTimeZone::parse("Europe/Stockholm").unwrap();
        let days = |month: &str| {
            let repository = &repository;
            let stockholm = &stockholm;
            let dates = AiBillingMonth::parse(month).unwrap().dates();
            async move { repository.local_days(dates, stockholm).await.unwrap() }
        };

        let march = days("2026-03").await;
        assert_eq!(march.len(), 31);
        assert_eq!(
            (march[0].day, march[0].start),
            (date!(2026 - 03 - 01), datetime!(2026-02-28 23:00 UTC))
        );
        let hours = |day: &AiLocalDay| (day.end - day.start).whole_hours();
        assert_eq!(hours(&march[28]), 23, "{:?}", march[28]);
        assert_eq!(march[30].end, datetime!(2026-03-31 22:00 UTC));
        let october = days("2026-10").await;
        assert_eq!(
            (october[24].day, hours(&october[24])),
            (date!(2026 - 10 - 25), 25)
        );
        assert!(october.windows(2).all(|pair| pair[0].end == pair[1].start));
    }

    #[sqlx::test]
    async fn machines_report_usage_latest_coverage_and_logged_syncs_of_the_month(pool: PgPool) {
        setup(
            &pool,
            (
                datetime!(2026-08-09 22:00 UTC),
                datetime!(2026-08-20 22:00 UTC),
            ),
            vec![bucket(datetime!(2026-08-12 08:00 UTC), MAPPED, 1, 0.1)],
        )
        .await;
        sqlx::query("UPDATE ai_usage_machines SET last_synced_at = '2026-08-20T21:30:00Z'")
            .execute(&pool)
            .await
            .unwrap();
        let repository = PostgresAiBillingRepository::new(db(&pool));
        let stockholm = AiUsageTimeZone::parse("Europe/Stockholm").unwrap();
        let month = |month: &str| AiBillingMonth::parse(month).unwrap().dates();

        let august = repository
            .machines(None, month("2026-08"), &stockholm)
            .await
            .unwrap();
        let [machine] = august.as_slice() else {
            panic!("expected one machine, got {august:?}");
        };
        assert_eq!(machine.machine_id.to_string(), MACHINE);
        // 23:30 in Stockholm: the local date, not the UTC one.
        assert_eq!(machine.last_synced_on, date!(2026 - 08 - 20));
        assert!(machine.synced_since_month_start);
        assert_eq!(machine.providers_with_usage, [AiProvider::Claude]);
        assert_eq!(
            machine.latest_reports,
            [AiProviderReport {
                provider: AiProvider::Claude,
                status: AiCoverageStatus::Partial,
                pricing_status: Some(AiPricingStatus::Cached),
            }]
        );
        // The upload happened after its window, so it covers all of it.
        assert_eq!(
            machine.intervals,
            [AiCoverageInterval {
                provider: AiProvider::Claude,
                start: datetime!(2026-08-09 22:00 UTC),
                end: datetime!(2026-08-20 22:00 UTC),
            }]
        );

        // July: synced after it began, but no usage or logged sync in it.
        let july = repository
            .machines(Some(&machine.user_id), month("2026-07"), &stockholm)
            .await
            .unwrap();
        assert!(july[0].synced_since_month_start);
        assert!(july[0].providers_with_usage.is_empty() && july[0].intervals.is_empty());
        let october = repository
            .machines(Some(&machine.user_id), month("2026-10"), &stockholm)
            .await
            .unwrap();
        assert!(!october[0].synced_since_month_start);
        assert!(repository
            .machines(
                Some(&UserId::new(machine.user_id.as_i32() + 1)),
                month("2026-08"),
                &stockholm
            )
            .await
            .unwrap()
            .is_empty());
    }
}

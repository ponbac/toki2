use async_trait::async_trait;

use crate::{
    db::DbPool,
    domain::{
        models::{
            AiMappedProject, AiTokenCounts, AiUsageDateRange, AiUsagePeriod, AiUsagePeriodTotals,
            AiUsageTimeZone, AiUsageTotals, AiUsageUpload, ProjectId, TimeTrackingCompany, UserId,
        },
        ports::outbound::AiUsageRepository,
        AiUsageError,
    },
};

pub struct PostgresAiUsageRepository {
    pool: DbPool,
}

impl PostgresAiUsageRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Whether the database's time zone data names `time_zone`. Checked at
    /// startup, so a misspelt setting fails there rather than in later queries.
    pub async fn knows_time_zone(&self, time_zone: &AiUsageTimeZone) -> Result<bool, AiUsageError> {
        sqlx::query_scalar!(
            r#"
            SELECT EXISTS (SELECT 1 FROM pg_timezone_names WHERE name = $1) AS "known!"
            "#,
            time_zone.as_str(),
        )
        .fetch_one(&self.pool)
        .await
        .map_err(storage_error)
    }
}

#[async_trait]
impl AiUsageRepository for PostgresAiUsageRepository {
    async fn replace_window(
        &self,
        user_id: &UserId,
        upload: &AiUsageUpload,
    ) -> Result<u64, AiUsageError> {
        let machine = upload.machine();
        let window = upload.window();
        let pricing = upload.pricing();
        let replaced = column(&upload.replaced_providers(), |p| p.as_str());

        let mut transaction = self.pool.begin().await.map_err(storage_error)?;

        // Registers the machine on first upload. Afterwards the conflict update
        // applies only for the owner, and its row lock serializes concurrent
        // uploads from one machine until commit.
        let registered = sqlx::query_scalar!(
            r#"
            INSERT INTO ai_usage_machines (id, user_id, label, client_version, time_zone)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (id) DO UPDATE
            SET label = EXCLUDED.label,
                client_version = EXCLUDED.client_version,
                time_zone = EXCLUDED.time_zone,
                last_synced_at = now()
            WHERE ai_usage_machines.user_id = EXCLUDED.user_id
            RETURNING id
            "#,
            machine.id.as_uuid(),
            user_id.as_i32(),
            machine.label,
            machine.client_version,
            machine.time_zone,
        )
        .fetch_optional(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        if registered.is_none() {
            return Err(AiUsageError::MachineOwnedByAnotherUser);
        }

        // Every reported provider updates its coverage, including missing and
        // failed ones; providers the upload does not mention keep theirs.
        let coverage = upload.coverage();
        sqlx::query!(
            r#"
            INSERT INTO ai_usage_machine_coverage (
                machine_id, provider, status, files, unreadable, malformed_lines,
                skipped_records, duplicates, window_start, window_end
            )
            SELECT $1, entry.provider, entry.status, entry.files, entry.unreadable,
                entry.malformed_lines, entry.skipped_records, entry.duplicates, $2, $3
            FROM UNNEST(
                $4::text[], $5::text[], $6::int8[], $7::int8[], $8::int8[], $9::int8[],
                $10::int8[]
            ) AS entry (
                provider, status, files, unreadable, malformed_lines, skipped_records, duplicates
            )
            ON CONFLICT (machine_id, provider) DO UPDATE
            SET status = EXCLUDED.status,
                files = EXCLUDED.files,
                unreadable = EXCLUDED.unreadable,
                malformed_lines = EXCLUDED.malformed_lines,
                skipped_records = EXCLUDED.skipped_records,
                duplicates = EXCLUDED.duplicates,
                window_start = EXCLUDED.window_start,
                window_end = EXCLUDED.window_end,
                reported_at = now()
            "#,
            machine.id.as_uuid(),
            window.start(),
            window.end(),
            &column(coverage, |c| c.provider.as_str()) as &[&str],
            &column(coverage, |c| c.status.as_str()) as &[&str],
            &column(coverage, |c| c.files),
            &column(coverage, |c| c.unreadable),
            &column(coverage, |c| c.malformed_lines),
            &column(coverage, |c| c.skipped_records),
            &column(coverage, |c| c.duplicates),
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        // Pricing describes stored costs, so only replaced providers take it.
        sqlx::query!(
            r#"
            UPDATE ai_usage_machine_coverage
            SET pricing_status = $3,
                pricing_fetched_at = $4,
                pricing_source = $5
            WHERE machine_id = $1
              AND provider = ANY($2::text[])
            "#,
            machine.id.as_uuid(),
            &replaced as &[&str],
            pricing.status.as_str(),
            pricing.fetched_at,
            pricing.source,
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        // Only providers covered as ok or partial are replaced: missing data is
        // not zero, so other providers' stored usage stays.
        sqlx::query!(
            r#"
            DELETE FROM ai_usage_buckets
            WHERE machine_id = $1
              AND provider = ANY($2::text[])
              AND hour_start >= $3
              AND hour_start < $4
            "#,
            machine.id.as_uuid(),
            &replaced as &[&str],
            window.start(),
            window.end(),
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        let buckets = upload.buckets();
        let hour_starts = column(buckets, |b| b.hour_start);
        let session_keys = column(buckets, |b| b.session_key.as_str());
        let project_keys = column(buckets, |b| b.project_key.as_str());
        let providers = column(buckets, |b| b.provider.as_str());
        let models = column(buckets, |b| b.model.as_str());
        let input_tokens = column(buckets, |b| b.tokens.input);
        let cache_read_tokens = column(buckets, |b| b.tokens.cache_read);
        let cache_write_tokens = column(buckets, |b| b.tokens.cache_write);
        let output_tokens = column(buckets, |b| b.tokens.output);
        let records = column(buckets, |b| b.records);
        let costs = column(buckets, |b| b.estimated_cost_usd);
        let unpriced_records = column(buckets, |b| b.unpriced_records);

        let inserted = sqlx::query!(
            r#"
            INSERT INTO ai_usage_buckets (
                user_id, machine_id, hour_start, session_key, project_key, provider, model,
                input_tokens, cache_read_tokens, cache_write_tokens, output_tokens,
                records, estimated_cost_usd, unpriced_records
            )
            SELECT
                $1, $2, bucket.hour_start, bucket.session_key, bucket.project_key,
                bucket.provider, bucket.model, bucket.input_tokens, bucket.cache_read_tokens,
                bucket.cache_write_tokens, bucket.output_tokens, bucket.records,
                bucket.estimated_cost_usd::numeric, bucket.unpriced_records
            FROM UNNEST(
                $3::timestamptz[], $4::text[], $5::text[], $6::text[], $7::text[],
                $8::int8[], $9::int8[], $10::int8[], $11::int8[], $12::int8[],
                $13::float8[], $14::int8[]
            ) AS bucket (
                hour_start, session_key, project_key, provider, model,
                input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, records,
                estimated_cost_usd, unpriced_records
            )
            "#,
            user_id.as_i32(),
            machine.id.as_uuid(),
            &hour_starts,
            &session_keys as &[&str],
            &project_keys as &[&str],
            &providers as &[&str],
            &models as &[&str],
            &input_tokens,
            &cache_read_tokens,
            &cache_write_tokens,
            &output_tokens,
            &records,
            &costs as &[Option<f64>],
            &unpriced_records,
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        sqlx::query!(
            r#"
            DELETE FROM ai_usage_provider_hints
            WHERE machine_id = $1
              AND provider = ANY($2::text[])
              AND hour_start >= $3
              AND hour_start < $4
            "#,
            machine.id.as_uuid(),
            &replaced as &[&str],
            window.start(),
            window.end(),
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        let hints = upload.provider_hints();
        sqlx::query!(
            r#"
            INSERT INTO ai_usage_provider_hints (user_id, machine_id, hour_start, provider, plan)
            SELECT $1, $2, hint.hour_start, hint.provider, hint.plan
            FROM UNNEST($3::timestamptz[], $4::text[], $5::text[])
                AS hint (hour_start, provider, plan)
            "#,
            user_id.as_i32(),
            machine.id.as_uuid(),
            &column(hints, |h| h.hour_start),
            &column(hints, |h| h.provider.as_str()) as &[&str],
            &column(hints, |h| h.plan.as_str()) as &[&str],
        )
        .execute(&mut transaction.executor())
        .await
        .map_err(storage_error)?;

        transaction.commit().await.map_err(storage_error)?;

        Ok(inserted.rows_affected())
    }

    async fn period_totals(
        &self,
        user_id: &UserId,
        dates: AiUsageDateRange,
        period: AiUsagePeriod,
        time_zone: &AiUsageTimeZone,
        mapping_company: Option<&TimeTrackingCompany>,
    ) -> Result<Vec<AiUsagePeriodTotals>, AiUsageError> {
        let unit = match period {
            AiUsagePeriod::Day => "day",
            AiUsagePeriod::Month => "month",
        };

        // Converting local midnights and hour starts in the zone keeps days and
        // months correct across daylight-saving changes. A bucket belongs to the
        // local date of its start instant. Periods are clipped to the range, and
        // the range bounds stay comparable with the (user_id, hour_start) index.
        // Mappings are joined here, never stored with usage, so they apply
        // retroactively. Only mappings to the configured company resolve, so
        // without one nothing does. Grouping by the mapping's key keeps one row
        // per project key.
        let rows = sqlx::query!(
            r#"
            SELECT
                greatest(period.start, $2::date) AS "start!",
                least((period.start + ('1 ' || $4)::interval)::date, $3::date) AS "end!",
                bucket.project_key,
                mapping.project_id AS "mapped_project_id?",
                mapping.project_name AS "mapped_project_name?",
                SUM(bucket.input_tokens)::int8 AS "input_tokens!",
                SUM(bucket.cache_read_tokens)::int8 AS "cache_read_tokens!",
                SUM(bucket.cache_write_tokens)::int8 AS "cache_write_tokens!",
                SUM(bucket.output_tokens)::int8 AS "output_tokens!",
                SUM(bucket.records)::int8 AS "records!",
                COALESCE(SUM(bucket.estimated_cost_usd), 0)::float8 AS "priced_cost_usd!",
                COUNT(*) FILTER (WHERE bucket.estimated_cost_usd IS NULL) AS "unpriced_buckets!",
                SUM(bucket.unpriced_records)::int8 AS "unpriced_records!"
            FROM ai_usage_buckets AS bucket
            CROSS JOIN LATERAL (
                SELECT date_trunc($4, bucket.hour_start AT TIME ZONE $5)::date AS start
            ) AS period
            LEFT JOIN ai_project_mappings AS mapping
                ON mapping.project_key = bucket.project_key
                AND mapping.provider = $6
                AND mapping.provider_company_id = $7
            WHERE bucket.user_id = $1
              AND bucket.hour_start >= ($2::date::timestamp AT TIME ZONE $5)
              AND bucket.hour_start < ($3::date::timestamp AT TIME ZONE $5)
            GROUP BY period.start, bucket.project_key, mapping.project_key
            ORDER BY period.start, bucket.project_key
            "#,
            user_id.as_i32(),
            dates.start(),
            dates.end(),
            unit,
            time_zone.as_str(),
            mapping_company.map(|company| company.provider.as_str()),
            mapping_company.map(|company| company.company_id.as_str()),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(numeric_read_error)?;

        rows.into_iter()
            .map(|row| {
                let dates = AiUsageDateRange::new(row.start, row.end).ok_or_else(|| {
                    AiUsageError::Storage("a usage period ends before it starts".to_string())
                })?;

                let project = match (row.mapped_project_id, row.mapped_project_name) {
                    (Some(id), Some(name)) => Some(AiMappedProject {
                        id: ProjectId::new(id),
                        name,
                    }),
                    (None, None) => None,
                    _ => {
                        return Err(AiUsageError::Storage(
                            "a project mapping has only one of its project id and name".to_string(),
                        ))
                    }
                };

                Ok(AiUsagePeriodTotals {
                    dates,
                    project_key: row.project_key,
                    project,
                    totals: AiUsageTotals {
                        tokens: AiTokenCounts {
                            input: row.input_tokens,
                            cache_read: row.cache_read_tokens,
                            cache_write: row.cache_write_tokens,
                            output: row.output_tokens,
                        },
                        records: row.records,
                        priced_cost_usd: row.priced_cost_usd,
                        unpriced_buckets: row.unpriced_buckets,
                        unpriced_records: row.unpriced_records,
                    }
                    .checked_for_report()?,
                })
            })
            .collect()
    }
}

/// One field of every row, for binding as a Postgres array.
fn column<'a, R, T>(rows: &'a [R], value: impl Fn(&'a R) -> T) -> Vec<T> {
    rows.iter().map(value).collect()
}

fn storage_error(error: sqlx::Error) -> AiUsageError {
    AiUsageError::Storage(error.to_string())
}

/// Numeric casts can fail even though every stored bucket was valid. Keep
/// representational failures separate from unavailable or corrupt storage.
pub(super) fn numeric_read_error(error: sqlx::Error) -> AiUsageError {
    if error
        .as_database_error()
        .and_then(|error| error.code())
        .is_some_and(|code| code == "22003")
    {
        AiUsageError::NumericRange
    } else {
        storage_error(error)
    }
}

#[cfg(test)]
mod tests;

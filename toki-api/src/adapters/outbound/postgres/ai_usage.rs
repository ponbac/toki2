use async_trait::async_trait;

use crate::{
    db::DbPool,
    domain::{
        models::{AiUsageTimeZone, AiUsageUpload, UserId},
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

        // The lock serializes uploads but not in the order their history was
        // read: a slow or retried upload could replace a later one with less.
        // Uploads that report their snapshot are applied only in snapshot order;
        // an equal snapshot is a retry of the stored upload. Snapshots compare
        // as the client reported them, so two from a fast clock keep their
        // order. One further ahead of the server than the tolerance is applied
        // but not remembered, so a wrong clock cannot lock its machine out of
        // later uploads for longer than the tolerance.
        if let Some(snapshot_at) = upload.snapshot_at() {
            let applied = sqlx::query_scalar!(
                r#"
                UPDATE ai_usage_machines
                SET last_snapshot_at = CASE
                    WHEN $2::timestamptz <= now() + interval '5 minutes' THEN $2::timestamptz
                    ELSE last_snapshot_at
                END
                WHERE id = $1
                  AND (last_snapshot_at IS NULL OR last_snapshot_at <= $2::timestamptz)
                RETURNING id
                "#,
                machine.id.as_uuid(),
                snapshot_at,
            )
            .fetch_optional(&mut transaction.executor())
            .await
            .map_err(storage_error)?;

            // Dropping the transaction rolls back the machine update above.
            if applied.is_none() {
                return Err(AiUsageError::SupersededUpload);
            }
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

        // Logs what the upload proves for each replaced provider: its stored
        // usage is complete from the window start until the earliest of the
        // window end, the snapshot and now, since later usage cannot be in the
        // upload. LEAST ignores a missing snapshot.
        sqlx::query!(
            r#"
            INSERT INTO ai_usage_coverage_log (
                machine_id, provider, window_start, covered_until, reported_at
            )
            SELECT $1, provider, $2, least($3::timestamptz, $5::timestamptz, now()), now()
            FROM UNNEST($4::text[]) AS provider
            WHERE $2::timestamptz < least($3::timestamptz, $5::timestamptz, now())
            "#,
            machine.id.as_uuid(),
            window.start(),
            window.end(),
            &replaced as &[&str],
            upload.snapshot_at(),
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
        let inserted = sqlx::query!(
            r#"
            INSERT INTO ai_usage_buckets (
                user_id, machine_id, hour_start, session_key, project_key, provider, model,
                input_tokens, cache_read_tokens, cache_write_tokens, output_tokens,
                records, estimated_cost_usd, unpriced_records, billing_provider
            )
            SELECT
                $1, $2, bucket.hour_start, bucket.session_key, bucket.project_key,
                bucket.provider, bucket.model, bucket.input_tokens, bucket.cache_read_tokens,
                bucket.cache_write_tokens, bucket.output_tokens, bucket.records,
                bucket.estimated_cost_usd::numeric, bucket.unpriced_records, bucket.billing_provider
            FROM UNNEST(
                $3::timestamptz[], $4::text[], $5::text[], $6::text[], $7::text[],
                $8::int8[], $9::int8[], $10::int8[], $11::int8[], $12::int8[],
                $13::float8[], $14::int8[], $15::text[]
            ) AS bucket (
                hour_start, session_key, project_key, provider, model,
                input_tokens, cache_read_tokens, cache_write_tokens, output_tokens, records,
                estimated_cost_usd, unpriced_records, billing_provider
            )
            "#,
            user_id.as_i32(),
            machine.id.as_uuid(),
            &column(buckets, |b| b.hour_start),
            &column(buckets, |b| b.session_key.as_str()) as &[&str],
            &column(buckets, |b| b.project_key.as_str()) as &[&str],
            &column(buckets, |b| b.provider.as_str()) as &[&str],
            &column(buckets, |b| b.model.as_str()) as &[&str],
            &column(buckets, |b| b.tokens.input),
            &column(buckets, |b| b.tokens.cache_read),
            &column(buckets, |b| b.tokens.cache_write),
            &column(buckets, |b| b.tokens.output),
            &column(buckets, |b| b.records),
            &column(buckets, |b| b.estimated_cost_usd) as &[Option<f64>],
            &column(buckets, |b| b.unpriced_records),
            &column(buckets, |b| b.billing_provider.map(|payer| payer.as_str()))
                as &[Option<&str>],
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

use std::collections::HashMap;

use async_trait::async_trait;
use time::{Date, Weekday};
use uuid::Uuid;

use super::{
    ai_usage::numeric_read_error,
    ai_usage_reads::{local_today, project_key_usage, stored_mappings},
};
use crate::{
    db::DbPool,
    domain::{
        models::{
            AiCoverageStatus, AiKeySelection, AiMachine, AiMachineCoverage, AiMachineId,
            AiMachineStatus, AiPricing, AiPricingStatus, AiProvider, AiProviderCoverage,
            AiSessionCursor, AiSessionOrder, AiStoredMapping, AiTokenCounts, AiUsageDateRange,
            AiUsageDayTotals, AiUsageHourOfWeekTotals, AiUsageMachineTotals, AiUsageModelTotals,
            AiUsageOptionSums, AiUsageScope, AiUsageSelection, AiUsageSessionRow,
            AiUsageSessionRows, AiUsageSums, AiUsageTimeZone, AiUsageTotals, ProjectId, UserId,
            MAX_SAFE_COUNT,
        },
        ports::outbound::AiUsageReportRepository,
        AiUsageError,
    },
};

/// Reads one user's own AI usage. Every query filters on the user, so no read
/// reaches another user's buckets or machines.
pub struct PostgresAiUsageReportRepository {
    pool: DbPool,
}

impl PostgresAiUsageReportRepository {
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }
}

// Every range query converts local midnights in the zone and compares them
// with `hour_start`, so the (user_id, hour_start) index applies and days
// follow daylight-saving changes. Costs are summed as NUMERIC and converted to
// float8 once per figure, as in `period_totals`, so no figure is a float sum of
// float sums. The store never decides attribution: selections pass the keys to
// keep and each resolving key's project.

/// Usage totals from a row whose sums have the usual column names.
macro_rules! totals {
    ($row:expr) => {
        AiUsageTotals {
            tokens: AiTokenCounts {
                input: $row.input_tokens,
                cache_read: $row.cache_read_tokens,
                cache_write: $row.cache_write_tokens,
                output: $row.output_tokens,
            },
            records: $row.records,
            priced_cost_usd: $row.priced_cost_usd,
            unpriced_buckets: $row.unpriced_buckets,
            unpriced_records: $row.unpriced_records,
        }
        .checked_for_report()?
    };
}

/// A selection's keys as the `only` and `except` query parameters.
fn key_parameters(keys: &AiKeySelection) -> (Option<Vec<String>>, Option<Vec<String>>) {
    match keys {
        AiKeySelection::All => (None, None),
        AiKeySelection::Only(keys) => (Some(keys.clone()), None),
        AiKeySelection::Except(keys) => (None, Some(keys.clone())),
    }
}

// GROUPING() bits of `usage_sums`, from its first grouping column to its last:
// a set bit means the row is not grouped by that column.
const BY_DATE: i32 = 1 << 7;
const BY_WEEKDAY: i32 = 1 << 6;
const BY_HOUR: i32 = 1 << 5;
const BY_MACHINE: i32 = 1 << 4;
const BY_PROVIDER: i32 = 1 << 3;
const BY_MODEL: i32 = 1 << 2;
const BY_PROJECT: i32 = 1 << 1;
const BY_KEY: i32 = 1;
const ALL_COLUMNS: i32 = (1 << 8) - 1;

/// The GROUPING() value of a row grouped by exactly `columns`.
const fn grouped_by(columns: i32) -> i32 {
    ALL_COLUMNS & !columns
}

#[async_trait]
impl AiUsageReportRepository for PostgresAiUsageReportRepository {
    async fn today(&self, time_zone: &AiUsageTimeZone) -> Result<Date, AiUsageError> {
        local_today(&self.pool, time_zone)
            .await
            .map_err(numeric_read_error)
    }

    async fn stored_mappings(&self) -> Result<Vec<AiStoredMapping>, AiUsageError> {
        stored_mappings(&self.pool)
            .await
            .map_err(numeric_read_error)
    }

    async fn usage_sums(
        &self,
        user_id: &UserId,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
        selection: &AiUsageSelection,
    ) -> Result<AiUsageSums, AiUsageError> {
        let (only, except) = key_parameters(&selection.keys);
        let (resolved_keys, resolved_projects): (Vec<&str>, Vec<&str>) = selection
            .projects
            .iter()
            .map(|(key, project)| (key.as_str(), project.as_str()))
            .unzip();

        // One pass sums every breakdown with GROUPING SETS. When clocks go back,
        // two UTC hours start at the same local hour and share its cell. The
        // empty grouping set yields the totals row even without usage.
        let rows = sqlx::query!(
            r#"
            WITH selected AS (
                SELECT
                    (bucket.hour_start AT TIME ZONE $4) AS local,
                    bucket.machine_id,
                    bucket.provider,
                    bucket.model,
                    bucket.project_key,
                    resolved.project_id,
                    bucket.input_tokens,
                    bucket.cache_read_tokens,
                    bucket.cache_write_tokens,
                    bucket.output_tokens,
                    bucket.records,
                    bucket.estimated_cost_usd,
                    bucket.unpriced_records
                FROM ai_usage_buckets AS bucket
                LEFT JOIN UNNEST($10::text[], $11::text[]) AS resolved (project_key, project_id)
                    ON resolved.project_key = bucket.project_key
                WHERE bucket.user_id = $1
                  AND bucket.hour_start >= ($2::date::timestamp AT TIME ZONE $4)
                  AND bucket.hour_start < ($3::date::timestamp AT TIME ZONE $4)
                  AND ($5::text IS NULL OR bucket.provider = $5)
                  AND ($6::text IS NULL OR bucket.model = $6)
                  AND ($7::uuid IS NULL OR bucket.machine_id = $7)
                  AND ($8::text[] IS NULL OR bucket.project_key = ANY($8))
                  AND ($9::text[] IS NULL OR bucket.project_key <> ALL($9))
            )
            SELECT
                GROUPING(
                    parts.date, parts.weekday, parts.hour, selected.machine_id,
                    selected.provider, selected.model, selected.project_id, selected.project_key
                ) AS "grouping!",
                parts.date AS "date?",
                parts.weekday AS "weekday?",
                parts.hour AS "hour?",
                selected.machine_id AS "machine_id?",
                selected.provider AS "provider?",
                selected.model AS "model?",
                selected.project_id AS "project_id?",
                selected.project_key AS "project_key?",
                COALESCE(SUM(selected.input_tokens), 0)::int8 AS "input_tokens!",
                COALESCE(SUM(selected.cache_read_tokens), 0)::int8 AS "cache_read_tokens!",
                COALESCE(SUM(selected.cache_write_tokens), 0)::int8 AS "cache_write_tokens!",
                COALESCE(SUM(selected.output_tokens), 0)::int8 AS "output_tokens!",
                COALESCE(SUM(selected.records), 0)::int8 AS "records!",
                COALESCE(SUM(selected.estimated_cost_usd), 0)::float8 AS "priced_cost_usd!",
                COUNT(*) FILTER (WHERE selected.estimated_cost_usd IS NULL) AS "unpriced_buckets!",
                COALESCE(SUM(selected.unpriced_records), 0)::int8 AS "unpriced_records!"
            FROM selected
            CROSS JOIN LATERAL (
                SELECT
                    selected.local::date AS date,
                    extract(isodow FROM selected.local)::int4 AS weekday,
                    extract(hour FROM selected.local)::int4 AS hour
            ) AS parts
            GROUP BY GROUPING SETS (
                (),
                (parts.date, selected.provider, selected.model),
                (selected.project_id),
                (selected.project_key),
                (selected.provider, selected.model),
                (selected.machine_id),
                (parts.weekday, parts.hour)
            )
            "#,
            user_id.as_i32(),
            dates.start(),
            dates.end(),
            time_zone.as_str(),
            selection.provider.map(AiProvider::as_str),
            selection.model.as_deref(),
            selection.machine.map(|machine| machine.as_uuid()),
            only.as_deref(),
            except.as_deref(),
            &resolved_keys as &[&str],
            &resolved_projects as &[&str],
        )
        .fetch_all(&self.pool)
        .await
        .map_err(numeric_read_error)?;

        let mut sums = AiUsageSums::default();
        for row in rows {
            let totals = totals!(row);
            let grouping = row.grouping;
            if grouping == grouped_by(0) {
                sums.totals = totals;
            } else if grouping == grouped_by(BY_DATE | BY_PROVIDER | BY_MODEL) {
                sums.days.push(AiUsageDayTotals {
                    date: row.date.ok_or_else(|| corrupt("a day row has no date"))?,
                    provider: provider(row.provider.as_deref())?,
                    model: row.model.ok_or_else(|| corrupt("a day row has no model"))?,
                    totals,
                });
            } else if grouping == grouped_by(BY_PROJECT) {
                sums.projects
                    .push((row.project_id.map(ProjectId::new), totals));
            } else if grouping == grouped_by(BY_KEY) {
                sums.keys.push((
                    row.project_key
                        .ok_or_else(|| corrupt("a key row has no project key"))?,
                    totals,
                ));
            } else if grouping == grouped_by(BY_PROVIDER | BY_MODEL) {
                sums.models.push(AiUsageModelTotals {
                    provider: provider(row.provider.as_deref())?,
                    model: row
                        .model
                        .ok_or_else(|| corrupt("a model row has no model"))?,
                    totals,
                });
            } else if grouping == grouped_by(BY_MACHINE) {
                sums.machines.push(AiUsageMachineTotals {
                    machine_id: AiMachineId::from_uuid(
                        row.machine_id
                            .ok_or_else(|| corrupt("a machine row has no machine"))?,
                    ),
                    totals,
                });
            } else if grouping == grouped_by(BY_WEEKDAY | BY_HOUR) {
                sums.hours.push(AiUsageHourOfWeekTotals {
                    weekday: weekday(row.weekday)?,
                    hour: row
                        .hour
                        .and_then(|hour| u8::try_from(hour).ok())
                        .filter(|hour| *hour < 24)
                        .ok_or_else(|| corrupt("an hour row has no hour of day"))?,
                    totals,
                });
            } else {
                return Err(corrupt("a usage row has an unknown grouping"));
            }
        }

        Ok(sums)
    }

    async fn option_sums(
        &self,
        user_id: &UserId,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
    ) -> Result<AiUsageOptionSums, AiUsageError> {
        let rows = sqlx::query!(
            r#"
            SELECT
                GROUPING(bucket.provider, bucket.model, bucket.project_key, bucket.machine_id)
                    AS "grouping!",
                bucket.provider AS "provider?",
                bucket.model AS "model?",
                bucket.project_key AS "project_key?",
                bucket.machine_id AS "machine_id?",
                SUM(bucket.input_tokens)::int8 AS "input_tokens!",
                SUM(bucket.cache_read_tokens)::int8 AS "cache_read_tokens!",
                SUM(bucket.cache_write_tokens)::int8 AS "cache_write_tokens!",
                SUM(bucket.output_tokens)::int8 AS "output_tokens!",
                SUM(bucket.records)::int8 AS "records!",
                COALESCE(SUM(bucket.estimated_cost_usd), 0)::float8 AS "priced_cost_usd!",
                COUNT(*) FILTER (WHERE bucket.estimated_cost_usd IS NULL) AS "unpriced_buckets!",
                SUM(bucket.unpriced_records)::int8 AS "unpriced_records!"
            FROM ai_usage_buckets AS bucket
            WHERE bucket.user_id = $1
              AND bucket.hour_start >= ($2::date::timestamp AT TIME ZONE $4)
              AND bucket.hour_start < ($3::date::timestamp AT TIME ZONE $4)
            GROUP BY GROUPING SETS (
                (bucket.provider), (bucket.model), (bucket.project_key), (bucket.machine_id)
            )
            "#,
            user_id.as_i32(),
            dates.start(),
            dates.end(),
            time_zone.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(numeric_read_error)?;

        let mut options = AiUsageOptionSums::default();
        for row in rows {
            let totals = totals!(row);
            // Each row is grouped by one column, whose GROUPING() bit is clear.
            match row.grouping {
                0b0111 => options.providers.push(provider(row.provider.as_deref())?),
                0b1011 => options.models.push((
                    row.model
                        .ok_or_else(|| corrupt("a model row has no model"))?,
                    totals,
                )),
                0b1101 => options.project_keys.push(
                    row.project_key
                        .ok_or_else(|| corrupt("a key row has no project key"))?,
                ),
                0b1110 => options.machines.push(AiMachineId::from_uuid(
                    row.machine_id
                        .ok_or_else(|| corrupt("a machine row has no machine"))?,
                )),
                _ => return Err(corrupt("an option row has an unknown grouping")),
            }
        }
        options.providers.sort();
        options.project_keys.sort();
        options.machines.sort_by_key(|machine| machine.as_uuid());

        Ok(options)
    }

    async fn sessions(
        &self,
        user_id: &UserId,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
        selection: &AiUsageSelection,
        order: AiSessionOrder,
        after: Option<&AiSessionCursor>,
        limit: usize,
    ) -> Result<AiUsageSessionRows, AiUsageError> {
        let (only, except) = key_parameters(&selection.keys);
        let order = match order {
            AiSessionOrder::Recent => "recent",
            AiSessionOrder::Cost => "cost",
            AiSessionOrder::Tokens => "tokens",
        };
        let limit = i64::try_from(limit).map_err(|_| corrupt("a session limit is too large"))?;

        // Sessions are summed and ranked in the database, and a page continues
        // strictly after the cursor's ordering values: every column breaks ties
        // in one direction, so pages neither skip nor repeat sessions.
        let rows = sqlx::query!(
            r#"
            WITH sessions AS (
                SELECT
                    bucket.machine_id,
                    bucket.provider,
                    bucket.session_key,
                    array_agg(DISTINCT bucket.project_key ORDER BY bucket.project_key)
                        AS project_keys,
                    array_agg(DISTINCT bucket.model ORDER BY bucket.model) AS models,
                    min(bucket.hour_start) AS first_hour,
                    max(bucket.hour_start) AS last_hour,
                    SUM(bucket.input_tokens)::int8 AS input_tokens,
                    SUM(bucket.cache_read_tokens)::int8 AS cache_read_tokens,
                    SUM(bucket.cache_write_tokens)::int8 AS cache_write_tokens,
                    SUM(bucket.output_tokens)::int8 AS output_tokens,
                    SUM(bucket.records)::int8 AS records,
                    COALESCE(SUM(bucket.estimated_cost_usd), 0) AS priced_cost_usd,
                    COUNT(*) FILTER (WHERE bucket.estimated_cost_usd IS NULL) AS unpriced_buckets,
                    SUM(bucket.unpriced_records)::int8 AS unpriced_records
                FROM ai_usage_buckets AS bucket
                WHERE bucket.user_id = $1
                  AND bucket.hour_start >= ($2::date::timestamp AT TIME ZONE $4)
                  AND bucket.hour_start < ($3::date::timestamp AT TIME ZONE $4)
                  AND ($5::text IS NULL OR bucket.provider = $5)
                  AND ($6::text IS NULL OR bucket.model = $6)
                  AND ($7::uuid IS NULL OR bucket.machine_id = $7)
                  AND ($8::text[] IS NULL OR bucket.project_key = ANY($8))
                  AND ($9::text[] IS NULL OR bucket.project_key <> ALL($9))
                GROUP BY bucket.machine_id, bucket.provider, bucket.session_key
            ),
            ranked AS (
                SELECT
                    sessions.*,
                    CASE $10::text
                        WHEN 'cost' THEN sessions.priced_cost_usd
                        WHEN 'tokens' THEN (
                            sessions.input_tokens + sessions.cache_read_tokens
                                + sessions.cache_write_tokens + sessions.output_tokens
                        )::numeric
                        ELSE extract(epoch FROM sessions.last_hour)
                    END AS rank
                FROM sessions
            )
            SELECT
                ranked.machine_id AS "machine_id!",
                ranked.provider AS "provider!",
                ranked.session_key AS "session_key!",
                ranked.project_keys AS "project_keys!",
                ranked.models AS "models!",
                ranked.first_hour AS "first_hour!",
                ranked.last_hour AS "last_hour!",
                ranked.input_tokens AS "input_tokens!",
                ranked.cache_read_tokens AS "cache_read_tokens!",
                ranked.cache_write_tokens AS "cache_write_tokens!",
                ranked.output_tokens AS "output_tokens!",
                ranked.records AS "records!",
                ranked.priced_cost_usd::float8 AS "priced_cost_usd!",
                ranked.unpriced_buckets AS "unpriced_buckets!",
                ranked.unpriced_records AS "unpriced_records!",
                ranked.rank::text AS "rank!",
                (SELECT count(*) FROM sessions) AS "total!"
            FROM ranked
            WHERE $11::numeric IS NULL
               OR (
                   ranked.rank, ranked.last_hour, ranked.first_hour, ranked.machine_id,
                   ranked.provider, ranked.session_key
               ) < ($11::numeric, $12, $13, $14, $15, $16)
            ORDER BY ranked.rank DESC, ranked.last_hour DESC, ranked.first_hour DESC,
                ranked.machine_id DESC, ranked.provider DESC, ranked.session_key DESC
            LIMIT $17
            "#,
            user_id.as_i32(),
            dates.start(),
            dates.end(),
            time_zone.as_str(),
            selection.provider.map(AiProvider::as_str),
            selection.model.as_deref(),
            selection.machine.map(|machine| machine.as_uuid()),
            only.as_deref(),
            except.as_deref(),
            order,
            after.map(|cursor| cursor.rank.as_str()) as Option<&str>,
            after.map(|cursor| cursor.last_hour),
            after.map(|cursor| cursor.first_hour),
            after.map(|cursor| cursor.machine_id.as_uuid()),
            after.map(|cursor| cursor.provider.as_str()),
            after.map(|cursor| cursor.session_key.as_str()),
            limit,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(numeric_read_error)?;

        let total = rows.first().map_or(Ok(0), |row| session_total(row.total))?;
        let rows = rows
            .into_iter()
            .map(|row| {
                Ok(AiUsageSessionRow {
                    machine_id: AiMachineId::from_uuid(row.machine_id),
                    provider: provider(Some(&row.provider))?,
                    totals: totals!(row),
                    session_key: row.session_key,
                    project_keys: row.project_keys,
                    models: row.models,
                    first_hour: row.first_hour,
                    last_hour: row.last_hour,
                    rank: row.rank,
                })
            })
            .collect::<Result<Vec<_>, AiUsageError>>()?;

        // Past the last page the count cannot come from a row; a page of
        // nothing after a cursor still has sessions before it.
        let total = if rows.is_empty() && after.is_some() {
            self.session_count(user_id, dates, time_zone, selection)
                .await?
        } else {
            total
        };

        Ok(AiUsageSessionRows { total, rows })
    }

    async fn machines(&self, user_id: &UserId) -> Result<Vec<AiMachineStatus>, AiUsageError> {
        let machines = sqlx::query!(
            r#"
            SELECT id, label, client_version, time_zone, first_seen_at, last_synced_at
            FROM ai_usage_machines
            WHERE user_id = $1
            ORDER BY last_synced_at DESC, id
            "#,
            user_id.as_i32(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(numeric_read_error)?;

        let coverage_rows = sqlx::query!(
            r#"
            SELECT
                coverage.machine_id,
                coverage.provider,
                coverage.status,
                coverage.files,
                coverage.unreadable,
                coverage.malformed_lines,
                coverage.skipped_records,
                coverage.duplicates,
                coverage.window_start,
                coverage.window_end,
                coverage.reported_at,
                coverage.pricing_status,
                coverage.pricing_fetched_at,
                coverage.pricing_source,
                EXISTS (
                    SELECT 1 FROM ai_usage_buckets AS bucket
                    WHERE bucket.machine_id = coverage.machine_id
                      AND bucket.provider = coverage.provider
                ) AS "has_usage!"
            FROM ai_usage_machine_coverage AS coverage
            JOIN ai_usage_machines AS machine ON machine.id = coverage.machine_id
            WHERE machine.user_id = $1
            "#,
            user_id.as_i32(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(numeric_read_error)?;

        let mut coverage: HashMap<Uuid, Vec<AiMachineCoverage>> = HashMap::new();
        for row in coverage_rows {
            let pricing = match (row.pricing_status, row.pricing_source) {
                (Some(status), Some(source)) => Some(AiPricing {
                    status: AiPricingStatus::parse(&status)
                        .ok_or_else(|| corrupt("a stored pricing status is unknown"))?,
                    fetched_at: row.pricing_fetched_at,
                    source,
                }),
                (None, None) => None,
                _ => return Err(corrupt("stored pricing has only a status or a source")),
            };
            coverage
                .entry(row.machine_id)
                .or_default()
                .push(AiMachineCoverage {
                    coverage: AiProviderCoverage {
                        provider: provider(Some(&row.provider))?,
                        status: AiCoverageStatus::parse(&row.status)
                            .ok_or_else(|| corrupt("a stored coverage status is unknown"))?,
                        files: row.files,
                        unreadable: row.unreadable,
                        malformed_lines: row.malformed_lines,
                        skipped_records: row.skipped_records,
                        duplicates: row.duplicates,
                    },
                    window_start: row.window_start,
                    window_end: row.window_end,
                    reported_at: row.reported_at,
                    pricing,
                    has_usage: row.has_usage,
                });
        }

        Ok(machines
            .into_iter()
            .map(|row| {
                let mut coverage = coverage.remove(&row.id).unwrap_or_default();
                coverage.sort_by_key(|entry| entry.coverage.provider);
                AiMachineStatus {
                    machine: AiMachine {
                        id: AiMachineId::from_uuid(row.id),
                        label: row.label,
                        client_version: row.client_version,
                        time_zone: row.time_zone,
                    },
                    first_seen_at: row.first_seen_at,
                    last_synced_at: row.last_synced_at,
                    coverage,
                }
            })
            .collect())
    }

    async fn machine_sums(
        &self,
        user_id: &UserId,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
    ) -> Result<Vec<(AiMachineId, AiUsageTotals)>, AiUsageError> {
        let rows = sqlx::query!(
            r#"
            SELECT
                bucket.machine_id,
                SUM(bucket.input_tokens)::int8 AS "input_tokens!",
                SUM(bucket.cache_read_tokens)::int8 AS "cache_read_tokens!",
                SUM(bucket.cache_write_tokens)::int8 AS "cache_write_tokens!",
                SUM(bucket.output_tokens)::int8 AS "output_tokens!",
                SUM(bucket.records)::int8 AS "records!",
                COALESCE(SUM(bucket.estimated_cost_usd), 0)::float8 AS "priced_cost_usd!",
                COUNT(*) FILTER (WHERE bucket.estimated_cost_usd IS NULL) AS "unpriced_buckets!",
                SUM(bucket.unpriced_records)::int8 AS "unpriced_records!"
            FROM ai_usage_buckets AS bucket
            WHERE bucket.user_id = $1
              AND bucket.hour_start >= ($2::date::timestamp AT TIME ZONE $4)
              AND bucket.hour_start < ($3::date::timestamp AT TIME ZONE $4)
            GROUP BY bucket.machine_id
            "#,
            user_id.as_i32(),
            dates.start(),
            dates.end(),
            time_zone.as_str(),
        )
        .fetch_all(&self.pool)
        .await
        .map_err(numeric_read_error)?;

        rows.into_iter()
            .map(|row| Ok((AiMachineId::from_uuid(row.machine_id), totals!(row))))
            .collect()
    }

    async fn project_keys(
        &self,
        user_id: &UserId,
        time_zone: &AiUsageTimeZone,
    ) -> Result<Vec<(String, Date)>, AiUsageError> {
        let keys = project_key_usage(&self.pool, AiUsageScope::User(*user_id), time_zone)
            .await
            .map_err(numeric_read_error)?;

        Ok(keys
            .into_iter()
            .map(|key| (key.project_key, key.last_used_on))
            .collect())
    }
}

impl PostgresAiUsageReportRepository {
    /// How many sessions a selection holds, for a page after the last one.
    async fn session_count(
        &self,
        user_id: &UserId,
        dates: AiUsageDateRange,
        time_zone: &AiUsageTimeZone,
        selection: &AiUsageSelection,
    ) -> Result<usize, AiUsageError> {
        let (only, except) = key_parameters(&selection.keys);
        let count = sqlx::query_scalar!(
            r#"
            SELECT count(DISTINCT (bucket.machine_id, bucket.provider, bucket.session_key))
                AS "count!"
            FROM ai_usage_buckets AS bucket
            WHERE bucket.user_id = $1
              AND bucket.hour_start >= ($2::date::timestamp AT TIME ZONE $4)
              AND bucket.hour_start < ($3::date::timestamp AT TIME ZONE $4)
              AND ($5::text IS NULL OR bucket.provider = $5)
              AND ($6::text IS NULL OR bucket.model = $6)
              AND ($7::uuid IS NULL OR bucket.machine_id = $7)
              AND ($8::text[] IS NULL OR bucket.project_key = ANY($8))
              AND ($9::text[] IS NULL OR bucket.project_key <> ALL($9))
            "#,
            user_id.as_i32(),
            dates.start(),
            dates.end(),
            time_zone.as_str(),
            selection.provider.map(AiProvider::as_str),
            selection.model.as_deref(),
            selection.machine.map(|machine| machine.as_uuid()),
            only.as_deref(),
            except.as_deref(),
        )
        .fetch_one(&self.pool)
        .await
        .map_err(numeric_read_error)?;

        session_total(count)
    }
}

fn provider(raw: Option<&str>) -> Result<AiProvider, AiUsageError> {
    raw.and_then(AiProvider::parse)
        .ok_or_else(|| corrupt("a stored provider is unknown"))
}

/// An ISO weekday number, 1 (Monday) to 7 (Sunday).
fn weekday(number: Option<i32>) -> Result<Weekday, AiUsageError> {
    let monday = Weekday::Monday;
    match number {
        Some(number @ 1..=7) => Ok((1..number).fold(monday, |day, _| day.next())),
        _ => Err(corrupt("an hour row has no weekday")),
    }
}

fn corrupt(message: &str) -> AiUsageError {
    AiUsageError::Storage(message.to_string())
}

fn session_total(count: i64) -> Result<usize, AiUsageError> {
    if !(0..=MAX_SAFE_COUNT).contains(&count) {
        return Err(AiUsageError::NumericRange);
    }
    usize::try_from(count).map_err(|_| AiUsageError::NumericRange)
}

#[cfg(test)]
pub(crate) mod tests;

use sqlx::PgPool;
use time::macros::{date, datetime};

use super::*;
use crate::{
    adapters::outbound::postgres::{
        PostgresAiMeteredUsageRepository, PostgresAiProjectMappingRepository,
    },
    domain::{
        models::{AiDecimal, AiMeteredOrgCharge, AiOrgChargeKind, AiProjectKey, AiUsageDateRange},
        ports::outbound::{AiMeteredUsageRepository, AiProjectMappingRepository},
    },
};

const COPILOT: AiProvider = AiProvider::Copilot;

fn db(pool: &PgPool) -> DbPool {
    DbPool::from(pool.clone())
}

fn repository(pool: &PgPool) -> PostgresAiRepositoryMappingRepository {
    PostgresAiRepositoryMappingRepository::new(db(pool))
}

fn company() -> TimeTrackingCompany {
    TimeTrackingCompany {
        provider: "kleer".to_string(),
        company_id: "company-1".to_string(),
    }
}

fn project(id: &str, name: &str) -> AiMappedProject {
    AiMappedProject {
        id: ProjectId::new(id),
        name: name.to_string(),
    }
}

async fn insert_user(pool: &PgPool, email: &str, full_name: &str) -> UserId {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, full_name, picture, access_token, roles)
         VALUES ($1, $2, '', '', ARRAY['Admin'])
         RETURNING id",
    )
    .bind(email)
    .bind(full_name)
    .fetch_one(pool)
    .await
    .unwrap();
    UserId::new(id)
}

fn charge(
    day: Date,
    product: &str,
    sku: &str,
    repository_name: &str,
    kind: AiOrgChargeKind,
    quantity: &str,
) -> AiMeteredOrgCharge {
    let dec = |raw: &str| AiDecimal::parse(raw).unwrap();
    AiMeteredOrgCharge {
        day,
        product: product.to_string(),
        sku: sku.to_string(),
        unit: "AICredits".to_string(),
        repository: repository_name.to_string(),
        kind,
        unit_price: dec("0.01"),
        quantity: dec(quantity),
        gross_amount: dec(quantity),
        discount_amount: AiDecimal::ZERO,
        net_amount: dec(quantity),
        reported_at: None,
    }
}

fn code_review(day: Date, repository_name: &str, quantity: &str) -> AiMeteredOrgCharge {
    charge(
        day,
        "copilot",
        "Copilot AI Credits",
        repository_name,
        AiOrgChargeKind::RepositoryCredits,
        quantity,
    )
}

/// Stores the organization's usage rows of October's first days.
async fn store_org_usage(pool: &PgPool, rows: &[AiMeteredOrgCharge]) {
    PostgresAiMeteredUsageRepository::new(db(pool))
        .replace_org_usage(
            COPILOT,
            AiUsageDateRange::from_inclusive(date!(2026 - 10 - 01), date!(2026 - 10 - 10)).unwrap(),
            rows,
            datetime!(2026-10-11 06:00 UTC),
        )
        .await
        .unwrap();
}

/// Runs `sql` and reports whether Postgres refused it.
async fn refused(pool: &PgPool, sql: &str) -> bool {
    sqlx::query(sqlx::AssertSqlSafe(sql.to_string()))
        .execute(pool)
        .await
        .is_err()
}

#[sqlx::test]
async fn constraints_refuse_what_the_service_never_writes(pool: PgPool) {
    let insert = |provider: &str, name: &str, project_id: &str| {
        format!(
            "INSERT INTO ai_provider_repository_mappings (provider, repository_name,
                time_tracking_provider, provider_company_id, project_id, project_name)
             VALUES ('{provider}', '{name}', 'kleer', 'company-1', '{project_id}', 'Client A')"
        )
    };
    assert!(refused(&pool, &insert("codex", "widget-api", "101")).await);
    assert!(refused(&pool, &insert("copilot", "", "101")).await);
    assert!(refused(&pool, &insert("copilot", &"a".repeat(201), "101")).await);
    assert!(refused(&pool, &insert("copilot", "widget-api", "")).await);
    assert!(!refused(&pool, &insert("copilot", &"a".repeat(200), "101")).await);
    assert!(!refused(&pool, &insert("copilot", "widget-api", "101")).await);
    // One mapping per repository, whatever its case.
    assert!(refused(&pool, &insert("copilot", "Widget-API", "202")).await);
    assert!(
        refused(
            &pool,
            "INSERT INTO ai_provider_repository_mappings (provider, repository_name,
                time_tracking_provider, provider_company_id, project_id, project_name)
             VALUES ('copilot', 'gadget-web', 'kleer', '', '101', 'Client A')"
        )
        .await
    );
}

#[sqlx::test]
async fn upserts_match_case_insensitively_and_keep_the_creator(pool: PgPool) {
    let repository = repository(&pool);
    let ada = insert_user(&pool, "ada@example.com", "Ada Lovelace").await;
    let grace = insert_user(&pool, "grace@example.com", "Grace Hopper").await;

    let created = repository
        .upsert(
            COPILOT,
            "Widget-API",
            &company(),
            &project("101", "Client A"),
            &ada,
        )
        .await
        .unwrap();
    assert_eq!(created.payer, COPILOT);
    assert_eq!(created.repository, "Widget-API");
    assert_eq!(created.company, company());
    assert_eq!(created.project, project("101", "Client A"));
    assert_eq!(
        created.created_by.as_ref().unwrap().full_name,
        "Ada Lovelace"
    );
    assert_eq!(created.updated_by, created.created_by);

    let replaced = repository
        .upsert(
            COPILOT,
            "widget-api",
            &company(),
            &project("202", "Client B"),
            &grace,
        )
        .await
        .unwrap();
    assert_eq!(replaced.repository, "widget-api");
    assert_eq!(replaced.project, project("202", "Client B"));
    assert_eq!(replaced.created_by.as_ref().unwrap().id, ada);
    assert_eq!(
        replaced.updated_by.as_ref().unwrap().full_name,
        "Grace Hopper"
    );
    assert_eq!(replaced.created_at, created.created_at);
    assert!(replaced.updated_at >= created.updated_at);
    repository
        .upsert(
            COPILOT,
            "gadget-web",
            &company(),
            &project("101", "Client A"),
            &ada,
        )
        .await
        .unwrap();
    let listed: Vec<String> = repository
        .list(COPILOT)
        .await
        .unwrap()
        .into_iter()
        .map(|mapping| mapping.repository)
        .collect();
    assert_eq!(listed, ["gadget-web", "widget-api"]);

    // A deleted user leaves the mapping, without editors.
    sqlx::query("DELETE FROM users WHERE id = $1")
        .bind(ada.as_i32())
        .execute(&pool)
        .await
        .unwrap();
    let mappings = repository.list(COPILOT).await.unwrap();
    assert_eq!(mappings[0].created_by, None);
    assert_eq!(mappings[0].updated_by, None);
    assert_eq!(mappings[1].created_by, None);
    assert_eq!(mappings[1].updated_by.as_ref().unwrap().id, grace);

    // Deleting matches case-insensitively and says whether anything was.
    let name = |raw: &str| AiRepositoryName::parse(raw).unwrap();
    assert!(repository
        .delete(COPILOT, &name("WIDGET-api"))
        .await
        .unwrap());
    assert!(!repository
        .delete(COPILOT, &name("widget-api"))
        .await
        .unwrap());
    assert_eq!(repository.list(COPILOT).await.unwrap().len(), 1);
}

#[sqlx::test]
async fn repository_credits_are_the_repository_rows_per_spelling_and_day(pool: PgPool) {
    let (first, second) = (date!(2026 - 10 - 02), date!(2026 - 10 - 03));
    store_org_usage(
        &pool,
        &[
            code_review(first, "widget-api", "12"),
            // Two SKUs' rows of one spelling and day sum.
            charge(
                first,
                "copilot",
                "copilot ai credits",
                "widget-api",
                AiOrgChargeKind::RepositoryCredits,
                "0.5",
            ),
            code_review(second, "Widget-API", "3"),
            code_review(second, "gadget-web", "8"),
            // Never a repository's credits: the IDE aggregate, the cloud agent
            // and Actions.
            charge(
                first,
                "copilot",
                "Copilot AI Credits",
                "",
                AiOrgChargeKind::AccountCreditsAggregate,
                "100",
            ),
            charge(
                first,
                "copilot",
                "Copilot Cloud Agent",
                "",
                AiOrgChargeKind::UntrackableCredits,
                "4",
            ),
            charge(
                first,
                "actions",
                "Actions Linux",
                "gadget-web",
                AiOrgChargeKind::Compute,
                "30",
            ),
        ],
    )
    .await;
    let mut credits = repository(&pool).repository_credits(COPILOT).await.unwrap();
    credits.sort_by(|a, b| (a.day, &a.repository).cmp(&(b.day, &b.repository)));
    let credit = |repository: &str, day: Date, centi: i64| AiRepositoryDayCredits {
        repository: repository.to_string(),
        day,
        credits: AiUsdNanos::new(centi * 100_000).unwrap(),
    };
    assert_eq!(
        credits,
        [
            credit("widget-api", first, 1_250),
            credit("Widget-API", second, 300),
            credit("gadget-web", second, 800),
        ]
    );
}

#[sqlx::test]
async fn key_mappings_are_every_stored_project_key_mapping(pool: PgPool) {
    let ada = insert_user(&pool, "ada@example.com", "Ada Lovelace").await;
    PostgresAiProjectMappingRepository::new(db(&pool))
        .upsert(
            &AiProjectKey::parse("github.com/example-org/widget-api").unwrap(),
            &company(),
            &project("101", "Client A"),
            &ada,
        )
        .await
        .unwrap();
    let keys = repository(&pool).key_mappings().await.unwrap();
    assert_eq!(
        keys,
        [AiStoredMapping {
            project_key: "github.com/example-org/widget-api".to_string(),
            company: company(),
            project: project("101", "Client A"),
        }]
    );
}

#[sqlx::test]
async fn the_bill_reads_every_repository_mapping_of_the_payer(pool: PgPool) {
    use crate::domain::models::AiStoredRepositoryMapping;
    let ada = insert_user(&pool, "ada@example.com", "Ada Lovelace").await;
    repository(&pool)
        .upsert(
            COPILOT,
            "Widget-API",
            &company(),
            &project("101", "Client A"),
            &ada,
        )
        .await
        .unwrap();
    let stored = PostgresAiMeteredUsageRepository::new(db(&pool))
        .repository_mappings(COPILOT)
        .await
        .unwrap();
    assert_eq!(
        stored,
        [AiStoredRepositoryMapping {
            repository: "Widget-API".to_string(),
            company: company(),
            project: project("101", "Client A"),
        }]
    );
}

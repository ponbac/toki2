use sqlx::PgPool;
use time::{
    macros::{date, datetime},
    OffsetDateTime,
};

use super::*;
use crate::{
    adapters::outbound::postgres::{
        ClockedAiRepositoryMappingRepository, PostgresAiMeteredUsageRepository,
        PostgresAiRepositoryMappingRepository,
    },
    db::DbPool,
    domain::{
        models::{
            AiDecimal, AiMeteredOrgCharge, AiOrgChargeKind, AiRepositoryMonthCredits,
            AiUsageDateRange, AiUsdNanos, Project,
        },
        ports::outbound::AiMeteredUsageRepository,
        TimeTrackingError,
    },
};

/// Active projects of one company, or an unreachable provider for `None`.
struct StaticProjects {
    company: TimeTrackingCompany,
    projects: Option<Vec<Project>>,
}

impl StaticProjects {
    fn of(company_id: &str) -> Self {
        Self {
            company: TimeTrackingCompany {
                provider: "kleer".to_string(),
                company_id: company_id.to_string(),
            },
            projects: Some(vec![
                Project::new("101", "Client A"),
                Project::new("202", "Client B"),
            ]),
        }
    }

    fn unreachable() -> Self {
        Self {
            projects: None,
            ..Self::of("company-1")
        }
    }
}

#[async_trait]
impl TimeTrackingProjectCatalog for StaticProjects {
    fn company(&self) -> &TimeTrackingCompany {
        &self.company
    }

    async fn active_projects(&self) -> Result<Vec<Project>, TimeTrackingError> {
        self.projects
            .clone()
            .ok_or_else(|| TimeTrackingError::unknown("provider unavailable"))
    }
}

/// The store's clock: the overview shows October and November.
const NOW: OffsetDateTime = datetime!(2026-11-03 09:00 UTC);

type Service = AiRepositoryMappingServiceImpl<ClockedAiRepositoryMappingRepository, StaticProjects>;

fn service(pool: &PgPool, projects: Option<StaticProjects>) -> Service {
    AiRepositoryMappingServiceImpl::new(
        Arc::new(ClockedAiRepositoryMappingRepository {
            inner: PostgresAiRepositoryMappingRepository::new(DbPool::from(pool.clone())),
            now: NOW,
        }),
        projects.map(Arc::new),
        AiProvider::Copilot,
        AiRepositoryMappingSettings {
            key_host: "github.com".to_string(),
            billed_from: AiBillingMonth::parse("2026-10"),
        },
    )
}

async fn insert_admin(pool: &PgPool) -> UserId {
    let id: i32 = sqlx::query_scalar(
        "INSERT INTO users (email, full_name, picture, access_token, roles)
         VALUES ('admin@example.com', 'Admin Example', '', '', ARRAY['Admin'])
         RETURNING id",
    )
    .fetch_one(pool)
    .await
    .unwrap();
    UserId::new(id)
}

/// The organization's rows of October's first days: code review billed to
/// `repositories`, 10 credits each on the 2nd.
async fn billed(pool: &PgPool, repositories: &[&str]) {
    let rows: Vec<AiMeteredOrgCharge> = repositories
        .iter()
        .map(|repository| AiMeteredOrgCharge {
            day: date!(2026 - 10 - 02),
            product: "copilot".to_string(),
            sku: "Copilot AI Credits".to_string(),
            unit: "AICredits".to_string(),
            repository: repository.to_string(),
            kind: AiOrgChargeKind::RepositoryCredits,
            unit_price: AiDecimal::parse("0.01").unwrap(),
            quantity: AiDecimal::parse("10").unwrap(),
            gross_amount: AiDecimal::parse("0.1").unwrap(),
            discount_amount: AiDecimal::ZERO,
            net_amount: AiDecimal::parse("0.1").unwrap(),
            reported_at: None,
        })
        .collect();
    PostgresAiMeteredUsageRepository::new(DbPool::from(pool.clone()))
        .replace_org_usage(
            AiProvider::Copilot,
            AiUsageDateRange::from_inclusive(date!(2026 - 10 - 01), date!(2026 - 10 - 05)).unwrap(),
            &rows,
            datetime!(2026-10-06 06:00 UTC),
        )
        .await
        .unwrap();
}

#[sqlx::test]
async fn only_a_billed_repository_gets_a_new_mapping(pool: PgPool) {
    let admin = insert_admin(&pool).await;
    billed(&pool, &["Widget-API"]).await;
    let service = service(&pool, Some(StaticProjects::of("company-1")));

    assert!(matches!(
        service
            .set_mapping("gadget-web", &ProjectId::new("101"), admin)
            .await,
        Err(AiRepositoryMappingError::UnknownRepository(name)) if name == "gadget-web"
    ));
    // Any case maps the repository, stored as the payer spells it.
    let entry = service
        .set_mapping("widget-api", &ProjectId::new("101"), admin)
        .await
        .unwrap();
    assert_eq!(entry.mapping.repository, "Widget-API");
    assert_eq!(entry.mapping.project.name, "Client A");
    assert_eq!(entry.resolution, AiMappingResolution::Resolves);
    assert!(matches!(
        service
            .set_mapping("widget-api", &ProjectId::new("999"), admin)
            .await,
        Err(AiRepositoryMappingError::UnknownProject(id)) if id == "999"
    ));
    assert!(matches!(
        service.set_mapping("", &ProjectId::new("101"), admin).await,
        Err(AiRepositoryMappingError::InvalidRepository(_))
    ));
}

#[sqlx::test]
async fn a_mapping_stays_editable_once_its_repository_is_no_longer_billed(pool: PgPool) {
    let admin = insert_admin(&pool).await;
    billed(&pool, &["widget-api"]).await;
    let service = service(&pool, Some(StaticProjects::of("company-1")));
    service
        .set_mapping("widget-api", &ProjectId::new("101"), admin)
        .await
        .unwrap();
    // GitHub's restated days no longer bill it.
    billed(&pool, &[]).await;

    let entry = service
        .set_mapping("WIDGET-API", &ProjectId::new("202"), admin)
        .await
        .unwrap();
    assert_eq!(entry.mapping.repository, "widget-api");
    assert_eq!(entry.mapping.project.name, "Client B");
    let overview = service.overview().await.unwrap();
    assert_eq!(overview.repositories.len(), 1);
    assert_eq!(overview.repositories[0].last_billed_on, None);

    service.delete_mapping("Widget-Api").await.unwrap();
    assert!(matches!(
        service.delete_mapping("widget-api").await,
        Err(AiRepositoryMappingError::NotFound)
    ));
    assert!(service.overview().await.unwrap().repositories.is_empty());
}

#[sqlx::test]
async fn without_projects_nothing_can_be_mapped(pool: PgPool) {
    let admin = insert_admin(&pool).await;
    billed(&pool, &["widget-api"]).await;

    let unconfigured = service(&pool, None);
    assert!(matches!(
        unconfigured
            .set_mapping("widget-api", &ProjectId::new("101"), admin)
            .await,
        Err(AiRepositoryMappingError::NotConfigured)
    ));
    let unreachable = service(&pool, Some(StaticProjects::unreachable()));
    assert!(matches!(
        unreachable
            .set_mapping("widget-api", &ProjectId::new("101"), admin)
            .await,
        Err(AiRepositoryMappingError::ProjectsUnavailable(_))
    ));

    // A mapping made while configured is unconfigured without time tracking,
    // and stale for another company.
    service(&pool, Some(StaticProjects::of("company-1")))
        .set_mapping("widget-api", &ProjectId::new("101"), admin)
        .await
        .unwrap();
    let resolution = |service: Service| async move {
        service.overview().await.unwrap().repositories[0]
            .mapping
            .as_ref()
            .unwrap()
            .resolution
    };
    assert_eq!(
        resolution(unconfigured).await,
        AiMappingResolution::Unconfigured
    );
    assert_eq!(
        resolution(service(&pool, Some(StaticProjects::of("company-2")))).await,
        AiMappingResolution::Stale
    );
}

#[sqlx::test]
async fn the_overview_shows_this_and_last_utc_month(pool: PgPool) {
    billed(&pool, &["widget-api"]).await;
    let overview = service(&pool, Some(StaticProjects::of("company-1")))
        .overview()
        .await
        .unwrap();
    let month = |raw: &str| AiBillingMonth::parse(raw).unwrap();
    // The store's clock picks the months, never the test's.
    assert_eq!(overview.months, [month("2026-10"), month("2026-11")]);
    assert_eq!(overview.billed_from, AiBillingMonth::parse("2026-10"));
    assert_eq!(overview.payer, AiProvider::Copilot);
    let row = &overview.repositories[0];
    assert_eq!(row.repository, "widget-api");
    assert_eq!(row.last_billed_on, Some(date!(2026 - 10 - 02)));
    // 10 credits of $0.01 in October, the cutover month, none in November.
    assert_eq!(
        row.months,
        [
            AiRepositoryMonthCredits {
                month: month("2026-10"),
                credits: AiUsdNanos::new(100_000_000).unwrap(),
                billed: true,
            },
            AiRepositoryMonthCredits {
                month: month("2026-11"),
                credits: AiUsdNanos::ZERO,
                billed: false,
            },
        ]
    );
}

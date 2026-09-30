use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use time::OffsetDateTime;

use crate::domain::{
    models::{
        report_dates, sessions_limit, AiLocalCalendar, AiMachineHealth, AiMachineList,
        AiMappingResolver, AiProjectAttribution, AiProjectKeyList, AiProjectKeyUsage,
        AiSessionCursor, AiSessionOrder, AiUsageDateRange, AiUsageReport, AiUsageReportQuery,
        AiUsageSelection, AiUsageSessionList, AiUsageTimeZone, TimeTrackingCompany, UserId,
    },
    ports::{inbound::AiUsageReportService, outbound::AiUsageReportRepository},
    AiUsageError,
};

pub struct AiUsageReportServiceImpl<R> {
    repository: Arc<R>,
    time_zone: AiUsageTimeZone,
    mapping_company: Option<TimeTrackingCompany>,
}

impl<R> AiUsageReportServiceImpl<R> {
    /// `time_zone` defines local days and hours. Only project mappings to
    /// `mapping_company`, the configured time-tracking company, resolve; with
    /// `None`, time tracking is not configured and all usage is unassigned.
    pub fn new(
        repository: Arc<R>,
        time_zone: AiUsageTimeZone,
        mapping_company: Option<TimeTrackingCompany>,
    ) -> Self {
        Self {
            repository,
            time_zone,
            mapping_company,
        }
    }
}

impl<R: AiUsageReportRepository> AiUsageReportServiceImpl<R> {
    async fn calendar_and_dates(
        &self,
        query: &AiUsageReportQuery,
    ) -> Result<(AiLocalCalendar, AiUsageDateRange), AiUsageError> {
        let today = self.repository.today(&self.time_zone).await?;
        let dates = report_dates(today, query.from, query.to)?;

        Ok((
            AiLocalCalendar {
                time_zone: self.time_zone.clone(),
                today,
            },
            dates,
        ))
    }

    async fn resolver(&self) -> Result<AiMappingResolver, AiUsageError> {
        Ok(AiMappingResolver::new(
            self.repository.stored_mappings().await?,
            self.mapping_company.clone(),
        ))
    }
}

#[async_trait]
impl<R: AiUsageReportRepository> AiUsageReportService for AiUsageReportServiceImpl<R> {
    async fn report(
        &self,
        user_id: &UserId,
        query: &AiUsageReportQuery,
    ) -> Result<AiUsageReport, AiUsageError> {
        let (calendar, dates) = self.calendar_and_dates(query).await?;
        let resolver = self.resolver().await?;
        let selection = AiUsageSelection::new(&query.filter, &resolver);
        let options = self
            .repository
            .option_sums(user_id, dates, &self.time_zone)
            .await?;
        let sums = self
            .repository
            .usage_sums(user_id, dates, &self.time_zone, &selection)
            .await?;

        Ok(AiUsageReport::assemble(
            calendar, dates, &resolver, options, sums,
        ))
    }

    async fn sessions(
        &self,
        user_id: &UserId,
        query: &AiUsageReportQuery,
        order: AiSessionOrder,
        after: Option<&str>,
        limit: Option<usize>,
    ) -> Result<AiUsageSessionList, AiUsageError> {
        let limit = sessions_limit(limit)?;
        let after = after
            .map(|cursor| AiSessionCursor::parse(cursor, order))
            .transpose()?;
        let (calendar, dates) = self.calendar_and_dates(query).await?;
        let resolver = self.resolver().await?;
        let selection = AiUsageSelection::new(&query.filter, &resolver);
        // One row more than the page shows whether another page follows.
        let rows = self
            .repository
            .sessions(
                user_id,
                dates,
                &self.time_zone,
                &selection,
                order,
                after.as_ref(),
                limit + 1,
            )
            .await?;

        Ok(AiUsageSessionList::page(
            calendar, dates, order, limit, rows, &resolver,
        ))
    }

    async fn machines(
        &self,
        user_id: &UserId,
        query: &AiUsageReportQuery,
    ) -> Result<AiMachineList, AiUsageError> {
        let (_, dates) = self.calendar_and_dates(query).await?;
        let now = OffsetDateTime::now_utc();
        let machines = self.repository.machines(user_id).await?;
        let mut usage = self
            .repository
            .machine_sums(user_id, dates, &self.time_zone)
            .await?
            .into_iter()
            .map(|(machine_id, totals)| (machine_id.as_uuid(), totals))
            .collect::<HashMap<_, _>>();

        Ok(AiMachineList {
            dates,
            machines: machines
                .into_iter()
                .map(|status| AiMachineHealth {
                    stale: status.is_stale_at(now),
                    usage: usage.remove(&status.machine.id.as_uuid()),
                    status,
                })
                .collect(),
        })
    }

    async fn project_keys(&self, user_id: &UserId) -> Result<AiProjectKeyList, AiUsageError> {
        let resolver = self.resolver().await?;
        let keys = self
            .repository
            .project_keys(user_id, &self.time_zone)
            .await?;

        Ok(AiProjectKeyList {
            time_tracking_configured: resolver.is_configured(),
            keys: keys
                .into_iter()
                .map(|(key, last_used_on)| AiProjectKeyUsage {
                    project: AiProjectAttribution::of(key, &resolver),
                    last_used_on,
                })
                .collect(),
        })
    }
}

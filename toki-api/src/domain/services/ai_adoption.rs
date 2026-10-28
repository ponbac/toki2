use std::sync::Arc;

use async_trait::async_trait;
use time::Date;

use crate::domain::{
    models::{
        adoption_dates, adoption_days, adoption_models, company_days, listed_people, AiAdoption,
        AiAdoptionCreditStatus, AiAdoptionCredits, AiAdoptionModel, AiUsageDateRange,
        AiUsageTimeZone, UserId,
    },
    ports::{inbound::AiAdoptionService, outbound::AiAdoptionRepository},
    AiBillingError,
};

pub struct AiAdoptionServiceImpl<R> {
    repository: Arc<R>,
    time_zone: AiUsageTimeZone,
    credits: Option<AiAdoptionCredits>,
}

impl<R> AiAdoptionServiceImpl<R> {
    /// `time_zone` defines local days. With `credits`, a metered payer's own
    /// credits measure its usage from its first imported day on.
    pub fn new(
        repository: Arc<R>,
        time_zone: AiUsageTimeZone,
        credits: Option<AiAdoptionCredits>,
    ) -> Self {
        Self {
            repository,
            time_zone,
            credits,
        }
    }
}

impl<R: AiAdoptionRepository> AiAdoptionServiceImpl<R> {
    async fn dates(
        &self,
        from: Option<Date>,
        to: Option<Date>,
    ) -> Result<(Date, AiUsageDateRange), AiBillingError> {
        let today = self.repository.today(&self.time_zone).await?;
        Ok((today, adoption_dates(today, from, to)?))
    }
}

#[async_trait]
impl<R: AiAdoptionRepository> AiAdoptionService for AiAdoptionServiceImpl<R> {
    async fn adoption(
        &self,
        from: Option<Date>,
        to: Option<Date>,
    ) -> Result<AiAdoption, AiBillingError> {
        let (today, dates) = self.dates(from, to).await?;
        let credits = self.credits.as_ref();
        let rows = self
            .repository
            .usage(dates, &self.time_zone, credits, None)
            .await?;
        let (user_credits, company_totals, complete_through) = match credits {
            Some(credits) => (
                self.repository.user_credits(credits, dates).await?,
                self.repository.company_credits(credits, dates).await?,
                self.repository
                    .credits_complete_through(credits, today)
                    .await?,
            ),
            None => (Vec::new(), Vec::new(), None),
        };
        let days = adoption_days(rows, &user_credits, credits.map(|credits| credits.payer))?;
        let people = listed_people(self.repository.people(today, &self.time_zone).await?, &days);

        Ok(AiAdoption {
            today,
            time_zone: self.time_zone.clone(),
            dates,
            people,
            days,
            company_days: credits
                .map(|credits| company_days(&company_totals, &user_credits, credits.payer))
                .unwrap_or_default(),
            credits: credits.map(|credits| AiAdoptionCreditStatus {
                payer: credits.payer,
                from: credits.from,
                complete_through,
            }),
        })
    }

    async fn models(
        &self,
        user_id: &UserId,
        from: Option<Date>,
        to: Option<Date>,
    ) -> Result<(AiUsageDateRange, Vec<AiAdoptionModel>), AiBillingError> {
        if !self.repository.user_exists(user_id).await? {
            return Err(AiBillingError::UserNotFound);
        }
        let (_, dates) = self.dates(from, to).await?;
        let rows = self
            .repository
            .usage(dates, &self.time_zone, self.credits.as_ref(), Some(user_id))
            .await?;
        Ok((dates, adoption_models(rows)?))
    }
}

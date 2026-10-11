use async_trait::async_trait;
use time::Date;

use super::AiExchangeRates;
use crate::domain::{
    models::{display_rate_currencies, display_rate_month, display_rates, AiDisplayRates},
    ports::{inbound::AiDisplayRateService, outbound::AiExchangeRateRepository},
    AiBillingError,
};

pub struct AiDisplayRateServiceImpl<X> {
    rates: AiExchangeRates<X>,
}

impl<X> AiDisplayRateServiceImpl<X> {
    /// Pass the `rates` that billing uses, so that both share stored rates and
    /// the bounds on fetching them.
    pub fn new(rates: AiExchangeRates<X>) -> Self {
        Self { rates }
    }
}

#[async_trait]
impl<X: AiExchangeRateRepository> AiDisplayRateService for AiDisplayRateServiceImpl<X> {
    async fn display_rates(&self, through: Option<Date>) -> Result<AiDisplayRates, AiBillingError> {
        let today = self.rates.today().await?;
        let month = display_rate_month(today, through)
            .ok_or_else(|| AiBillingError::Invalid("the range ends in no month".to_string()))?;
        let currencies = display_rate_currencies();

        let found = self.rates.month_rates(month, &currencies).await?;
        let mut pending = !found.pending.is_empty();
        let rates = display_rates(month, &found.rates);
        if !rates.is_empty() {
            return Ok(AiDisplayRates {
                month,
                rates,
                pending,
            });
        }

        // No USD rate for the month (yet): the month before stands in.
        if let Some(previous) = month.previous() {
            let found = self.rates.month_rates(previous, &currencies).await?;
            pending |= !found.pending.is_empty();
            let rates = display_rates(previous, &found.rates);
            if !rates.is_empty() {
                return Ok(AiDisplayRates {
                    month: previous,
                    rates,
                    pending,
                });
            }
        }

        Ok(AiDisplayRates {
            month,
            rates: Vec::new(),
            pending,
        })
    }
}

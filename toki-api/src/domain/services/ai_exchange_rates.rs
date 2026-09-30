//! A month's exchange rates for billing: stored rates, and fetches of the ones
//! that are missing or due, outside the request that needs them.
//!
//! - **Single flight.** Each month and currency has at most one fetch in
//!   flight; concurrent bills wait on the same one.
//! - **Bounded.** At most `concurrency` fetches call the provider at once.
//! - **Capped wait.** A bill waits at most `wait` for its fetches. The fetches
//!   carry on in the background and store their rates; the bill shows what is
//!   stored and reports the rest as pending.
//! - **Provider backoff.** After a rate limit or a provider-wide failure (a
//!   timeout, a server error, a missing endpoint), no rate is asked for until
//!   the provider's `Retry-After`, or else `retry_after`, has passed. It is
//!   logged once.
//! - **Rate backoff.** A rate the provider has not published, or answered
//!   unreadably, is not asked for again for `retry_after`.
//! - **Storage failures** do not fail a bill: the fetched rate is used for the
//!   request that fetched it, and fetched again later.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, MutexGuard, Weak,
    },
    time::{Duration, Instant},
};

use futures::future::{join_all, BoxFuture, FutureExt, Shared};
use tokio::sync::Semaphore;

use crate::domain::{
    models::{
        rate_to_fetch, AiBillingMonth, AiCurrency, AiExchangeRate, AiExchangeRateValue,
        AiFetchedExchangeRate, AiProvidedRate, AiRateClock, AiRateFetch, AiUsageTimeZone, UserId,
    },
    ports::outbound::{AiExchangeRateRepository, AiFetchedRateWrite, ExchangeRateProvider},
    AiBillingError, ExchangeRateError,
};

/// How fetching rates is bounded.
#[derive(Debug, Clone, Copy)]
pub struct AiExchangeRateSettings {
    /// Fetches that may call the provider at once.
    pub concurrency: usize,
    /// How long a bill waits for its fetches.
    pub wait: Duration,
    /// How long to leave a failed rate, or a failing provider that did not
    /// say, alone.
    pub retry_after: Duration,
}

impl Default for AiExchangeRateSettings {
    fn default() -> Self {
        Self {
            concurrency: 2,
            wait: Duration::from_secs(4),
            retry_after: Duration::from_secs(15 * 60),
        }
    }
}

/// A month's rates for a bill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiMonthRates {
    pub rates: Vec<AiExchangeRate>,
    /// Currencies whose fetch did not finish within the wait.
    pub pending: Vec<AiCurrency>,
}

/// How a fetch ended.
#[derive(Debug, Clone)]
enum FetchOutcome {
    /// A rate, and whether it was stored.
    Fetched {
        write: AiFetchedRateWrite,
        stored: bool,
    },
    /// No rate: not published, failed, or skipped while the provider backs off.
    NoRate,
}

type FetchKey = (String, String);
type InFlight = Shared<BoxFuture<'static, FetchOutcome>>;

#[derive(Default)]
struct Flights {
    fetching: HashMap<FetchKey, InFlight>,
    /// Only active snapshot readers retain a completion counter for their keys.
    /// Weak references are pruned on registration and fetch completion.
    completed: HashMap<FetchKey, Weak<AtomicU64>>,
}

#[derive(Default)]
struct FetchState {
    flights: Mutex<Flights>,
    /// When each rate last failed to fetch.
    failed: Mutex<HashMap<FetchKey, Instant>>,
    /// Until when the provider as a whole is left alone.
    provider_blocked_until: Mutex<Option<Instant>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub struct AiExchangeRates<X> {
    repository: Arc<X>,
    provider: Arc<dyn ExchangeRateProvider>,
    time_zone: AiUsageTimeZone,
    settings: AiExchangeRateSettings,
    permits: Arc<Semaphore>,
    state: Arc<FetchState>,
}

impl<X> Clone for AiExchangeRates<X> {
    fn clone(&self) -> Self {
        Self {
            repository: self.repository.clone(),
            provider: self.provider.clone(),
            time_zone: self.time_zone.clone(),
            settings: self.settings,
            permits: self.permits.clone(),
            state: self.state.clone(),
        }
    }
}

impl<X: AiExchangeRateRepository> AiExchangeRates<X> {
    pub fn new(
        repository: Arc<X>,
        provider: Arc<dyn ExchangeRateProvider>,
        time_zone: AiUsageTimeZone,
        settings: AiExchangeRateSettings,
    ) -> Self {
        Self {
            repository,
            provider,
            time_zone,
            permits: Arc::new(Semaphore::new(settings.concurrency.max(1))),
            settings,
            state: Arc::new(FetchState::default()),
        }
    }

    fn key(month: AiBillingMonth, currency: &AiCurrency) -> FetchKey {
        (month.to_string(), currency.as_str().to_string())
    }

    fn provider_blocked(&self) -> bool {
        lock(&self.state.provider_blocked_until).is_some_and(|until| Instant::now() < until)
    }

    /// Leaves the provider alone for as long as it asked, or `retry_after`.
    /// Logs once per backoff.
    fn block_provider(&self, error: &ExchangeRateError) {
        let wait = error.retry_after().unwrap_or(self.settings.retry_after);
        let now = Instant::now();
        let mut blocked = lock(&self.state.provider_blocked_until);
        let already = blocked.is_some_and(|until| now < until);
        let until = now + wait;
        *blocked = Some(blocked.filter(|current| *current > until).unwrap_or(until));
        if !already {
            tracing::warn!(
                "exchange rate provider {} failed, not asking it for {}s: {error}",
                self.provider.name(),
                wait.as_secs()
            );
        }
    }

    fn recently_failed(&self, month: AiBillingMonth, currency: &AiCurrency) -> bool {
        lock(&self.state.failed)
            .get(&Self::key(month, currency))
            .is_some_and(|at| at.elapsed() < self.settings.retry_after)
    }

    fn record_fetch(&self, month: AiBillingMonth, currency: &AiCurrency, succeeded: bool) {
        let mut failed = lock(&self.state.failed);
        let key = Self::key(month, currency);
        if succeeded {
            failed.remove(&key);
        } else {
            failed.insert(key, Instant::now());
        }
    }

    /// Asks the provider for a month's rate: `(rate, provisional)`.
    async fn fetch_rate(
        &self,
        currency: &AiCurrency,
        month: AiBillingMonth,
        fetch: AiRateFetch,
    ) -> Result<Option<(AiProvidedRate, bool)>, ExchangeRateError> {
        let provider = &self.provider;
        match fetch {
            AiRateFetch::MonthAverage => {
                if let Some(rate) = provider.monthly_average(currency, month).await? {
                    return Ok(Some((rate, false)));
                }
                // Not published yet: the month's days, provisionally.
                let rate = provider
                    .daily_average(currency, month.first_day(), month.last_day())
                    .await?;
                Ok(rate.map(|rate| (rate, true)))
            }
            AiRateFetch::MonthToDate { through } => {
                let rate = provider
                    .daily_average(currency, month.first_day(), through)
                    .await?;
                Ok(rate.map(|rate| (rate, true)))
            }
        }
    }

    /// Fetches a rate and stores it, within the concurrency bound.
    async fn fetch_and_store(
        &self,
        month: AiBillingMonth,
        currency: AiCurrency,
        fetch: AiRateFetch,
    ) -> FetchOutcome {
        let Ok(_permit) = self.permits.acquire().await else {
            return FetchOutcome::NoRate;
        };
        // The provider may have failed while this fetch waited its turn.
        if self.provider_blocked() {
            return FetchOutcome::NoRate;
        }
        match self.fetch_rate(&currency, month, fetch).await {
            Ok(Some((provided, provisional))) => {
                let write = AiFetchedRateWrite {
                    month,
                    currency: currency.clone(),
                    provided,
                    provider: self.provider.name(),
                    provisional,
                };
                let stored = match self.repository.store_fetched(&write).await {
                    Ok(()) => true,
                    Err(error) => {
                        tracing::error!(
                            "could not store the fetched {} exchange rate for {month}: {error}",
                            currency.as_str()
                        );
                        false
                    }
                };
                self.record_fetch(month, &currency, true);
                FetchOutcome::Fetched { write, stored }
            }
            Ok(None) => {
                tracing::info!(
                    "no {} exchange rate is published for {month} yet",
                    currency.as_str()
                );
                self.record_fetch(month, &currency, false);
                FetchOutcome::NoRate
            }
            Err(error) if error.is_provider_wide() => {
                self.block_provider(&error);
                FetchOutcome::NoRate
            }
            Err(error) => {
                tracing::warn!(
                    "could not fetch the {} exchange rate for {month}: {error}",
                    currency.as_str()
                );
                self.record_fetch(month, &currency, false);
                FetchOutcome::NoRate
            }
        }
    }

    /// The fetch of a month's rate in flight, started if there is none. It
    /// runs as its own task, so it finishes even if nobody waits for it.
    fn start(
        &self,
        flights: &mut Flights,
        month: AiBillingMonth,
        currency: &AiCurrency,
        fetch: AiRateFetch,
    ) -> InFlight {
        let key = Self::key(month, currency);
        if let Some(fetching) = flights.fetching.get(&key) {
            return fetching.clone();
        }
        let this = self.clone();
        let (task_key, currency) = (key.clone(), currency.clone());
        // Removal waits for the caller's flight lock to be released.
        let task = tokio::spawn(async move {
            let outcome = this.fetch_and_store(month, currency, fetch).await;
            let mut flights = lock(&this.state.flights);
            flights.fetching.remove(&task_key);
            if let Some(completed) = flights.completed.get(&task_key).and_then(Weak::upgrade) {
                completed.fetch_add(1, Ordering::Relaxed);
            }
            flights
                .completed
                .retain(|_, epoch| epoch.strong_count() > 0);
            outcome
        });
        let fetching = async move { task.await.unwrap_or(FetchOutcome::NoRate) }
            .boxed()
            .shared();
        flights.fetching.insert(key, fetching.clone());
        fetching
    }

    /// The month's stored rates, after fetching those of `currencies` that are
    /// missing or due (`rate_to_fetch`), waiting at most `settings.wait`.
    pub async fn month_rates(
        &self,
        month: AiBillingMonth,
        currencies: &[AiCurrency],
    ) -> Result<AiMonthRates, AiBillingError> {
        let (stored, clock, fetches) = loop {
            let completed: Vec<(Arc<AtomicU64>, u64)> = {
                let mut flights = lock(&self.state.flights);
                flights
                    .completed
                    .retain(|_, epoch| epoch.strong_count() > 0);
                currencies
                    .iter()
                    .map(|currency| {
                        let epoch = flights
                            .completed
                            .entry(Self::key(month, currency))
                            .or_default();
                        let counter = epoch.upgrade().unwrap_or_else(|| {
                            let counter = Arc::new(AtomicU64::new(0));
                            *epoch = Arc::downgrade(&counter);
                            counter
                        });
                        let observed = counter.load(Ordering::Relaxed);
                        (counter, observed)
                    })
                    .collect()
            };
            let stored = self.repository.month_rates(month, &self.time_zone).await?;
            let clock = self.repository.clock(&self.time_zone).await?;
            let fetches = {
                let mut flights = lock(&self.state.flights);
                // A relevant fetch may have stored a rate and left the map
                // while these reads awaited. Reload before deciding to start
                // work, under the same lock that records fetch completion.
                if completed
                    .iter()
                    .any(|(counter, observed)| counter.load(Ordering::Relaxed) != *observed)
                {
                    None
                } else {
                    Some(
                        currencies
                            .iter()
                            .filter_map(|currency| {
                                let current = stored.iter().find(|rate| &rate.currency == currency);
                                let fetch = rate_to_fetch(current, month, clock, |day| {
                                    self.provider.may_have_published_after(day, clock.now)
                                })?;
                                // Backoff stops new calls, while existing
                                // fetches remain pending until they finish.
                                if let Some(fetching) =
                                    flights.fetching.get(&Self::key(month, currency))
                                {
                                    return Some((currency.clone(), fetching.clone()));
                                }
                                (!self.provider_blocked() && !self.recently_failed(month, currency))
                                    .then(|| {
                                        (
                                            currency.clone(),
                                            self.start(&mut flights, month, currency, fetch),
                                        )
                                    })
                            })
                            .collect::<Vec<(AiCurrency, InFlight)>>(),
                    )
                }
            };
            if let Some(fetches) = fetches {
                break (stored, clock, fetches);
            }
        };
        if fetches.is_empty() {
            return Ok(AiMonthRates {
                rates: stored,
                pending: Vec::new(),
            });
        }
        let waiting = join_all(fetches.iter().map(|(_, fetching)| fetching.clone()));
        let _ = tokio::time::timeout(self.settings.wait, waiting).await;

        let mut pending = Vec::new();
        let mut unstored = Vec::new();
        let mut fetched_any = false;
        for (currency, fetching) in &fetches {
            match fetching.peek() {
                None => pending.push(currency.clone()),
                Some(FetchOutcome::Fetched { write, stored }) => {
                    fetched_any = true;
                    if !stored {
                        unstored.push(write.clone());
                    }
                }
                Some(FetchOutcome::NoRate) => {}
            }
        }
        let mut rates = if fetched_any {
            self.repository.month_rates(month, &self.time_zone).await?
        } else {
            stored
        };
        for write in unstored {
            use_unstored(&mut rates, write, clock);
        }

        Ok(AiMonthRates { rates, pending })
    }

    /// Overrides a month's rate. `Invalid` for the billing currency.
    pub async fn set_override(
        &self,
        month: AiBillingMonth,
        currency: &AiCurrency,
        rate: AiExchangeRateValue,
        by: &UserId,
    ) -> Result<AiExchangeRate, AiBillingError> {
        convertible(currency)?;
        self.repository
            .set_override(month, currency, rate, by)
            .await?;
        self.repository
            .month_rates(month, &self.time_zone)
            .await?
            .into_iter()
            .find(|stored| &stored.currency == currency)
            .ok_or_else(|| AiBillingError::Storage("the override was not stored".to_string()))
    }

    /// Removes an override. The stored fetched rate applies again; it is
    /// fetched only if missing or due. `Invalid` for the billing currency.
    pub async fn reset(
        &self,
        month: AiBillingMonth,
        currency: &AiCurrency,
    ) -> Result<Option<AiExchangeRate>, AiBillingError> {
        convertible(currency)?;
        self.repository.remove_override(month, currency).await?;
        // Whatever failed before, try this rate again now.
        self.record_fetch(month, currency, true);
        Ok(self
            .month_rates(month, std::slice::from_ref(currency))
            .await?
            .rates
            .into_iter()
            .find(|stored| &stored.currency == currency))
    }
}

/// Uses a fetched rate that could not be stored, for this bill only.
fn use_unstored(rates: &mut Vec<AiExchangeRate>, write: AiFetchedRateWrite, clock: AiRateClock) {
    let fetched = AiFetchedExchangeRate {
        rate: write.provided.rate,
        provider: write.provider.to_string(),
        provisional: write.provisional,
        observed_through: write.provided.observed_through,
        fetched_at: clock.now,
        fetched_on: clock.today,
    };
    match rates
        .iter_mut()
        .find(|rate| rate.currency == write.currency)
    {
        Some(rate) => rate.fetched = Some(fetched),
        None => rates.push(AiExchangeRate {
            month: write.month,
            currency: write.currency,
            fetched: Some(fetched),
            overridden: None,
        }),
    }
}

/// Refuses the billing currency, which has no rate.
fn convertible(currency: &AiCurrency) -> Result<(), AiBillingError> {
    if *currency == AiCurrency::billing() {
        return Err(AiBillingError::Invalid(format!(
            "{} is the billing currency and has no exchange rate",
            currency.as_str()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests;

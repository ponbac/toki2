use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use time::{macros::datetime, Date, OffsetDateTime};

use super::*;
use crate::domain::models::AiExchangeRateOverride;

/// Pauses one dependency call at a known point in a concurrent request.
#[derive(Default)]
struct Pause {
    reached: tokio::sync::Notify,
    resume: tokio::sync::Notify,
}

impl Pause {
    async fn wait(&self) {
        self.reached.notify_one();
        self.resume.notified().await;
    }

    async fn reached(&self) {
        tokio::time::timeout(Duration::from_secs(5), self.reached.notified())
            .await
            .expect("the dependency call reaches the pause");
    }
}

/// Stored rates in memory, whose writes can be made to fail.
struct MemoryRates {
    rates: Mutex<Vec<AiExchangeRate>>,
    fail_writes: bool,
    now: OffsetDateTime,
    clock_pause: Mutex<Option<Arc<Pause>>>,
    reads: AtomicUsize,
}

impl MemoryRates {
    fn new(fail_writes: bool) -> Arc<Self> {
        Arc::new(Self {
            rates: Mutex::new(Vec::new()),
            fail_writes,
            now: datetime!(2026-10-05 12:00 UTC),
            clock_pause: Mutex::new(None),
            reads: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl AiExchangeRateRepository for MemoryRates {
    async fn month_rates(
        &self,
        month: AiBillingMonth,
        _: &AiUsageTimeZone,
    ) -> Result<Vec<AiExchangeRate>, AiBillingError> {
        self.reads.fetch_add(1, Ordering::Relaxed);
        Ok(lock(&self.rates)
            .iter()
            .filter(|rate| rate.month == month)
            .cloned()
            .collect())
    }

    async fn store_fetched(&self, write: &AiFetchedRateWrite) -> Result<(), AiBillingError> {
        if self.fail_writes {
            return Err(AiBillingError::Storage("synthetic failure".to_string()));
        }
        let mut rates = lock(&self.rates);
        let clock = AiRateClock {
            now: self.now,
            today: self.now.date(),
        };
        use_unstored(&mut rates, write.clone(), clock);
        Ok(())
    }

    async fn set_override(
        &self,
        month: AiBillingMonth,
        currency: &AiCurrency,
        rate: AiExchangeRateValue,
        _: &UserId,
    ) -> Result<(), AiBillingError> {
        let overridden = Some(AiExchangeRateOverride {
            rate,
            set_on: self.now.date(),
            by: None,
        });
        let mut rates = lock(&self.rates);
        match rates.iter_mut().find(|stored| &stored.currency == currency) {
            Some(stored) => stored.overridden = overridden,
            None => rates.push(AiExchangeRate {
                month,
                currency: currency.clone(),
                fetched: None,
                overridden,
            }),
        }
        Ok(())
    }

    async fn remove_override(
        &self,
        _: AiBillingMonth,
        currency: &AiCurrency,
    ) -> Result<(), AiBillingError> {
        let mut rates = lock(&self.rates);
        for stored in rates
            .iter_mut()
            .filter(|stored| &stored.currency == currency)
        {
            stored.overridden = None;
        }
        rates.retain(|stored| stored.fetched.is_some());
        Ok(())
    }

    async fn clock(&self, _: &AiUsageTimeZone) -> Result<AiRateClock, AiBillingError> {
        let pause = lock(&self.clock_pause).take();
        if let Some(pause) = pause {
            pause.wait().await;
        }
        Ok(AiRateClock {
            now: self.now,
            today: self.now.date(),
        })
    }
}

/// A provider whose answers are set per currency, optionally slow.
#[derive(Default)]
struct FakeProvider {
    answers: Mutex<HashMap<String, Result<Option<AiProvidedRate>, ExchangeRateError>>>,
    delay: Duration,
    delays: HashMap<String, Duration>,
    calls: Mutex<Vec<String>>,
    response_pause: Mutex<Option<Arc<Pause>>>,
}

impl FakeProvider {
    fn delay_for(mut self, currency: &str, delay: Duration) -> Self {
        self.delays.insert(currency.to_string(), delay);
        self
    }

    fn answer(
        self,
        currency: &str,
        answer: Result<Option<AiProvidedRate>, ExchangeRateError>,
    ) -> Self {
        lock(&self.answers).insert(currency.to_string(), answer);
        self
    }

    fn calls(&self) -> Vec<String> {
        lock(&self.calls).clone()
    }

    async fn answer_for(
        &self,
        call: String,
        currency: &AiCurrency,
    ) -> Result<Option<AiProvidedRate>, ExchangeRateError> {
        lock(&self.calls).push(call);
        let pause = lock(&self.response_pause).take();
        if let Some(pause) = pause {
            pause.wait().await;
        }
        tokio::time::sleep(*self.delays.get(currency.as_str()).unwrap_or(&self.delay)).await;
        lock(&self.answers)
            .get(currency.as_str())
            .cloned()
            .unwrap_or(Ok(None))
    }
}

#[async_trait]
impl ExchangeRateProvider for FakeProvider {
    fn name(&self) -> &'static str {
        "example"
    }

    fn may_have_published_after(&self, _: Date, _: OffsetDateTime) -> bool {
        true
    }

    async fn monthly_average(
        &self,
        currency: &AiCurrency,
        month: AiBillingMonth,
    ) -> Result<Option<AiProvidedRate>, ExchangeRateError> {
        self.answer_for(format!("monthly {} {month}", currency.as_str()), currency)
            .await
    }

    async fn daily_average(
        &self,
        currency: &AiCurrency,
        from: Date,
        through: Date,
    ) -> Result<Option<AiProvidedRate>, ExchangeRateError> {
        self.answer_for(
            format!("daily {} {from} {through}", currency.as_str()),
            currency,
        )
        .await
    }
}

fn september() -> AiBillingMonth {
    AiBillingMonth::parse("2026-09").unwrap()
}

fn currency(code: &str) -> AiCurrency {
    AiCurrency::parse(code).unwrap()
}

fn provided(rate: &str) -> Result<Option<AiProvidedRate>, ExchangeRateError> {
    Ok(Some(AiProvidedRate {
        rate: AiExchangeRateValue::parse(rate).unwrap(),
        observed_through: time::macros::date!(2026 - 09 - 30),
    }))
}

fn rates(
    repository: Arc<MemoryRates>,
    provider: Arc<FakeProvider>,
    settings: AiExchangeRateSettings,
) -> AiExchangeRates<MemoryRates> {
    AiExchangeRates::new(
        repository,
        provider,
        AiUsageTimeZone::parse("Europe/Stockholm").unwrap(),
        settings,
    )
}

fn settings() -> AiExchangeRateSettings {
    AiExchangeRateSettings {
        concurrency: 1,
        wait: Duration::from_secs(5),
        retry_after: Duration::from_secs(60 * 60),
    }
}

fn billed_at(rates: &AiMonthRates) -> Vec<(String, String)> {
    rates
        .rates
        .iter()
        .map(|rate| {
            (
                rate.currency.as_str().to_string(),
                rate.rate().unwrap().to_decimal(),
            )
        })
        .collect()
}

fn pair(currency: &str, rate: &str) -> (String, String) {
    (currency.to_string(), rate.to_string())
}

#[tokio::test]
async fn concurrent_bills_share_one_fetch_per_rate() {
    let provider = Arc::new(
        FakeProvider {
            delay: Duration::from_millis(100),
            ..FakeProvider::default()
        }
        .answer("USD", provided("10.5"))
        .answer("EUR", provided("11.5")),
    );
    let service = rates(
        MemoryRates::new(false),
        provider.clone(),
        AiExchangeRateSettings {
            concurrency: 2,
            ..settings()
        },
    );
    let wanted = [currency("EUR"), currency("USD")];

    let (first, second, third) = tokio::join!(
        service.month_rates(september(), &wanted),
        service.month_rates(september(), &wanted),
        service.month_rates(september(), &wanted[..1]),
    );
    for bill in [first.unwrap(), second.unwrap()] {
        assert_eq!(
            billed_at(&bill),
            [pair("EUR", "11.50"), pair("USD", "10.50")]
        );
        assert!(bill.pending.is_empty());
    }
    assert!(billed_at(&third.unwrap()).contains(&pair("EUR", "11.50")));
    let mut calls = provider.calls();
    calls.sort();
    assert_eq!(calls, ["monthly EUR 2026-09", "monthly USD 2026-09"]);
}

#[tokio::test]
async fn a_fetch_completed_after_the_snapshot_is_not_started_again() {
    let provider_pause = Arc::new(Pause::default());
    let provider = Arc::new(
        FakeProvider {
            response_pause: Mutex::new(Some(provider_pause.clone())),
            ..FakeProvider::default()
        }
        .answer("USD", provided("10.5")),
    );
    let repository = MemoryRates::new(false);
    let service = rates(repository.clone(), provider.clone(), settings());
    let first_service = service.clone();
    let first = tokio::spawn(async move {
        first_service
            .month_rates(september(), &[currency("USD")])
            .await
            .unwrap()
    });
    provider_pause.reached().await;

    // The second bill reads no stored rate, then pauses in the clock call.
    let clock_pause = Arc::new(Pause::default());
    *lock(&repository.clock_pause) = Some(clock_pause.clone());
    let second_service = service.clone();
    let second = tokio::spawn(async move {
        second_service
            .month_rates(september(), &[currency("USD")])
            .await
            .unwrap()
    });
    clock_pause.reached().await;

    // The first fetch stores its final rate and leaves single-flight state
    // before the second bill continues from its obsolete snapshot.
    provider_pause.resume.notify_one();
    let first_bill = first.await.unwrap();
    assert_eq!(billed_at(&first_bill), [pair("USD", "10.50")]);
    assert!(first_bill.pending.is_empty());
    clock_pause.resume.notify_one();
    let second_bill = second.await.unwrap();
    assert_eq!(billed_at(&second_bill), [pair("USD", "10.50")]);
    assert!(second_bill.pending.is_empty());
    assert_eq!(provider.calls(), ["monthly USD 2026-09"]);

    // A later ordinary read also reuses the final persisted rate.
    let later = service
        .month_rates(september(), &[currency("USD")])
        .await
        .unwrap();
    assert_eq!(billed_at(&later), [pair("USD", "10.50")]);
    assert_eq!(provider.calls(), ["monthly USD 2026-09"]);
}

#[tokio::test]
async fn unrelated_completions_do_not_restart_a_rates_snapshot() {
    let mut redundant_reads = Vec::new();
    for (other_month, other_currency) in [
        (september(), currency("EUR")),
        (AiBillingMonth::parse("2026-08").unwrap(), currency("USD")),
    ] {
        let provider = Arc::new(FakeProvider::default().answer("USD", provided("10.5")));
        let repository = MemoryRates::new(false);
        let service = rates(repository.clone(), provider.clone(), settings());
        let cached = service
            .month_rates(september(), &[currency("USD")])
            .await
            .unwrap();
        assert_eq!(billed_at(&cached), [pair("USD", "10.50")]);

        // An unrelated missing rate completes with no published answer.
        // The already-final September USD rate remains usable throughout.
        lock(&provider.answers).insert(other_currency.as_str().to_string(), Ok(None));
        let provider_pause = Arc::new(Pause::default());
        *lock(&provider.response_pause) = Some(provider_pause.clone());
        let unrelated_service = service.clone();
        let unrelated = tokio::spawn(async move {
            unrelated_service
                .month_rates(other_month, &[other_currency])
                .await
                .unwrap()
        });
        provider_pause.reached().await;

        let clock_pause = Arc::new(Pause::default());
        *lock(&repository.clock_pause) = Some(clock_pause.clone());
        let target_service = service.clone();
        let target = tokio::spawn(async move {
            target_service
                .month_rates(september(), &[currency("USD")])
                .await
                .unwrap()
        });
        clock_pause.reached().await;

        provider_pause.resume.notify_one();
        assert!(unrelated.await.unwrap().pending.is_empty());
        let reads_before_resume = repository.reads.load(Ordering::Relaxed);
        clock_pause.resume.notify_one();
        let bill = target.await.unwrap();
        assert_eq!(billed_at(&bill), [pair("USD", "10.50")]);
        assert!(bill.pending.is_empty());
        redundant_reads.push(repository.reads.load(Ordering::Relaxed) - reads_before_resume);
        // Only the priming request called the target month/currency.
        assert_eq!(
            provider
                .calls()
                .iter()
                .filter(|call| call.as_str() == "monthly USD 2026-09")
                .count(),
            1
        );
    }
    // Neither another currency nor another month invalidates this reader.
    assert_eq!(redundant_reads, [0, 0]);
}

#[tokio::test]
async fn a_provider_wide_failure_stops_every_call_until_the_backoff_ends() {
    for failure in [
        ExchangeRateError::RateLimited { retry_after: None },
        ExchangeRateError::Unavailable("404 Not Found".to_string()),
    ] {
        let provider = Arc::new(
            FakeProvider::default()
                .answer("EUR", Err(failure.clone()))
                .answer("GBP", provided("13"))
                .answer("USD", provided("10.5")),
        );
        let service = rates(MemoryRates::new(false), provider.clone(), settings());
        let wanted = [currency("EUR"), currency("GBP"), currency("USD")];

        let bill = service.month_rates(september(), &wanted).await.unwrap();
        // The first failure stopped the fetches waiting their turn.
        assert!(bill.rates.is_empty(), "{failure}");
        assert!(bill.pending.is_empty());
        assert_eq!(provider.calls(), ["monthly EUR 2026-09"], "{failure}");

        // Within the backoff, no call at all.
        service.month_rates(september(), &wanted).await.unwrap();
        assert_eq!(provider.calls().len(), 1, "{failure}");
    }
}

#[tokio::test]
async fn provider_backoff_keeps_existing_fetches_pending_until_they_finish() {
    let provider = Arc::new(
        FakeProvider::default()
            .delay_for("USD", Duration::from_millis(300))
            .answer("USD", provided("10.5"))
            .answer(
                "EUR",
                Err(ExchangeRateError::RateLimited { retry_after: None }),
            ),
    );
    let service = rates(
        MemoryRates::new(false),
        provider.clone(),
        AiExchangeRateSettings {
            concurrency: 2,
            wait: Duration::from_millis(20),
            ..settings()
        },
    );
    let wanted = [currency("USD"), currency("EUR")];
    let first = service.month_rates(september(), &wanted).await.unwrap();
    assert_eq!(first.pending, [currency("USD")]);
    assert_eq!(provider.calls().len(), 2);

    let during_backoff = service.month_rates(september(), &wanted).await.unwrap();
    assert_eq!(during_backoff.pending, [currency("USD")]);
    assert_eq!(provider.calls().len(), 2);

    tokio::time::sleep(Duration::from_millis(350)).await;
    let finished = service.month_rates(september(), &wanted).await.unwrap();
    assert_eq!(billed_at(&finished), [pair("USD", "10.50")]);
    assert!(finished.pending.is_empty());
    assert_eq!(provider.calls().len(), 2);
}

#[tokio::test]
async fn retry_after_is_honoured() {
    let provider = Arc::new(FakeProvider::default().answer(
        "USD",
        Err(ExchangeRateError::RateLimited {
            retry_after: Some(Duration::from_millis(50)),
        }),
    ));
    let service = rates(MemoryRates::new(false), provider.clone(), settings());
    let wanted = [currency("USD")];
    service.month_rates(september(), &wanted).await.unwrap();
    service.month_rates(september(), &wanted).await.unwrap();
    assert_eq!(provider.calls().len(), 1);

    // After the provider's 50 ms, not the default hour, it is asked again.
    tokio::time::sleep(Duration::from_millis(80)).await;
    lock(&provider.answers).insert("USD".to_string(), provided("10.5"));
    let bill = service.month_rates(september(), &wanted).await.unwrap();
    assert_eq!(billed_at(&bill), [pair("USD", "10.50")]);
    assert_eq!(provider.calls().len(), 2);
}

#[tokio::test]
async fn a_bill_does_not_wait_past_its_cap() {
    let provider = Arc::new(
        FakeProvider {
            delay: Duration::from_millis(300),
            ..FakeProvider::default()
        }
        .answer("USD", provided("10.5")),
    );
    let repository = MemoryRates::new(false);
    let service = rates(
        repository.clone(),
        provider.clone(),
        AiExchangeRateSettings {
            wait: Duration::from_millis(30),
            ..settings()
        },
    );
    let wanted = [currency("USD")];

    let bill = service.month_rates(september(), &wanted).await.unwrap();
    assert!(bill.rates.is_empty());
    assert_eq!(bill.pending, [currency("USD")]);

    // The fetch finishes in the background and is stored.
    tokio::time::sleep(Duration::from_millis(400)).await;
    let bill = service.month_rates(september(), &wanted).await.unwrap();
    assert_eq!(billed_at(&bill), [pair("USD", "10.50")]);
    assert!(bill.pending.is_empty());
    assert_eq!(provider.calls().len(), 1);
}

#[tokio::test]
async fn a_rate_that_cannot_be_stored_is_still_used() {
    let provider = Arc::new(FakeProvider::default().answer("USD", provided("10.5")));
    let service = rates(MemoryRates::new(true), provider.clone(), settings());
    let bill = service
        .month_rates(september(), &[currency("USD")])
        .await
        .unwrap();
    assert_eq!(billed_at(&bill), [pair("USD", "10.50")]);
    assert!(!bill.rates[0].fetched.as_ref().unwrap().provisional);
}

#[tokio::test]
async fn a_reset_while_the_provider_fails_falls_back_to_the_fetched_rate() {
    let provider = Arc::new(FakeProvider::default().answer("USD", provided("10.5")));
    let repository = MemoryRates::new(false);
    let service = rates(repository.clone(), provider.clone(), settings());
    let usd = currency("USD");
    service
        .month_rates(september(), std::slice::from_ref(&usd))
        .await
        .unwrap();
    service
        .set_override(
            september(),
            &usd,
            AiExchangeRateValue::parse("11").unwrap(),
            &UserId::new(1),
        )
        .await
        .unwrap();

    lock(&provider.answers).insert(
        "USD".to_string(),
        Err(ExchangeRateError::Unavailable("down".to_string())),
    );
    let reset = service.reset(september(), &usd).await.unwrap().unwrap();
    assert_eq!(reset.rate().unwrap().to_decimal(), "10.50");
    assert_eq!(reset.source(), "example");
    // The final fetched rate was not due, so the provider was not asked.
    assert_eq!(provider.calls(), ["monthly USD 2026-09"]);
}

#[tokio::test]
async fn the_billing_currency_has_no_rate_to_override() {
    let service = rates(
        MemoryRates::new(false),
        Arc::new(FakeProvider::default()),
        settings(),
    );
    let sek = AiCurrency::billing();
    assert!(matches!(
        service
            .set_override(
                september(),
                &sek,
                AiExchangeRateValue::parse("1").unwrap(),
                &UserId::new(1)
            )
            .await,
        Err(AiBillingError::Invalid(_))
    ));
    assert!(matches!(
        service.reset(september(), &sek).await,
        Err(AiBillingError::Invalid(_))
    ));
}

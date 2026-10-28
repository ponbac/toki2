//! How much AI each person and the company use, for the admin Adoption tab.
//!
//! Everything here stops at one local day per person and tool: adoption never
//! sees hours or sessions, as no admin view of another developer's usage does.
//! Code completions are not usage: Copilot's free models (next edit
//! suggestions and its helper model) are left out wherever Copilot pays.
//!
//! Costs are API-equivalent USD estimates of the priced records, except where
//! a metered payer's own credits measure the usage instead (`AiAdoptionCredits`):
//! from that payer's first imported day on, its buckets keep their tokens and
//! records but carry no estimate, and the credits GitHub charged to the account
//! of the person on each UTC day are their cost. Credits no person owns that
//! day (unlinked, external and unknown accounts, and code review billed per
//! repository) count for the company only.

use std::collections::{BTreeMap, HashMap, HashSet};

use time::{Date, Month, OffsetDateTime};

use super::{
    metered_model_status, AiBillingUsage, AiMeteredModelStatus, AiProvider, AiUsageDateRange,
    AiUsageTimeZone, AiUsdNanos, UserId,
};
use crate::domain::AiBillingError;

/// The longest range one adoption read covers, in days.
pub const MAX_ADOPTION_DAYS: i64 = 400;

/// How many months back a read without `from` starts: the first day of the
/// month this many months before today's.
pub const DEFAULT_ADOPTION_MONTHS: u8 = 6;

/// From which UTC day a metered payer's own credits measure its usage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiAdoptionCredits {
    pub payer: AiProvider,
    /// The first UTC day with imported credits.
    pub from: Date,
}

impl AiAdoptionCredits {
    /// `from` at 00:00 UTC: buckets the payer pays for from here on are
    /// measured by its credits.
    pub fn starts_at(&self) -> OffsetDateTime {
        self.from.midnight().assume_utc()
    }
}

/// One person's usage of one model on one local day, as stored.
#[derive(Debug, Clone, PartialEq)]
pub struct AiAdoptionUsageRow {
    pub user_id: UserId,
    pub day: Date,
    /// The tool the usage came from.
    pub provider: AiProvider,
    /// Who paid for it, when not the tool itself.
    pub billing_provider: Option<AiProvider>,
    pub model: String,
    /// Whether these buckets are measured by the payer's credits: paid by
    /// `AiAdoptionCredits::payer` from its first instant on.
    pub credited: bool,
    pub usage: AiBillingUsage,
}

impl AiAdoptionUsageRow {
    /// Whether this is code completion rather than use of AI: one of a metered
    /// payer's free models, which it charges nothing for.
    fn is_completion(&self) -> bool {
        let payer = self.billing_provider.unwrap_or(self.provider);
        metered_model_status(payer, &self.model) == Some(AiMeteredModelStatus::Free)
    }

    /// The usage as adoption counts it: credited buckets keep their tokens
    /// and records, but the payer's credits are their cost, not the estimate.
    fn measured_usage(&self) -> AiBillingUsage {
        if self.credited {
            AiBillingUsage {
                priced_cost: AiUsdNanos::ZERO,
                unpriced_records: 0,
                ..self.usage
            }
        } else {
            self.usage
        }
    }
}

/// Credits charged to a person's account on one UTC day.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiAdoptionCredit {
    pub user_id: UserId,
    pub day: Date,
    pub amount: AiUsdNanos,
}

/// A seat someone holds at a payer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiAdoptionSeat {
    pub provider: AiProvider,
    pub since: Date,
}

/// Someone who may be listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAdoptionPerson {
    pub user_id: UserId,
    pub full_name: String,
    /// Linked to a time-tracking user: listed even without usage.
    pub linked: bool,
    /// The local day their first machine first synced usage.
    pub first_synced_on: Option<Date>,
    pub seats: Vec<AiAdoptionSeat>,
}

/// One person's use of one tool on one local day.
#[derive(Debug, Clone, PartialEq)]
pub struct AiAdoptionDay {
    pub user_id: UserId,
    pub day: Date,
    pub tool: AiProvider,
    /// Tokens and records from the person's machines. `priced_cost` is the
    /// estimate of the usage no credits measure plus the credits charged to
    /// the person; `unpriced_records` are the estimated usage's only.
    pub usage: AiBillingUsage,
}

/// Credits on one UTC day that no person's day carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiAdoptionCompanyDay {
    pub day: Date,
    pub tool: AiProvider,
    pub amount: AiUsdNanos,
}

/// How far a metered payer's credits are imported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiAdoptionCreditStatus {
    pub payer: AiProvider,
    pub from: Date,
    /// The last UTC day whose credits are complete; later days may still grow.
    pub complete_through: Option<Date>,
}

/// Everyone's AI use over a range of local days.
#[derive(Debug, Clone, PartialEq)]
pub struct AiAdoption {
    pub today: Date,
    pub time_zone: AiUsageTimeZone,
    pub dates: AiUsageDateRange,
    pub people: Vec<AiAdoptionPerson>,
    pub days: Vec<AiAdoptionDay>,
    pub company_days: Vec<AiAdoptionCompanyDay>,
    pub credits: Option<AiAdoptionCreditStatus>,
}

/// One person's use of one model over a range, completions left out.
#[derive(Debug, Clone, PartialEq)]
pub struct AiAdoptionModel {
    pub tool: AiProvider,
    pub model: String,
    pub usage: AiBillingUsage,
}

/// The range an adoption read covers: `from` through `to` inclusive, by
/// default from the first day of the month `DEFAULT_ADOPTION_MONTHS` before
/// today's through today.
pub fn adoption_dates(
    today: Date,
    from: Option<Date>,
    to: Option<Date>,
) -> Result<AiUsageDateRange, AiBillingError> {
    let to = to.unwrap_or(today);
    let from = match from {
        Some(from) => from,
        None => months_before(to, DEFAULT_ADOPTION_MONTHS)?,
    };
    if from > to {
        return Err(AiBillingError::Invalid(
            "from must not be after to".to_string(),
        ));
    }
    let dates = AiUsageDateRange::from_inclusive(from, to)
        .ok_or_else(|| AiBillingError::Invalid("from or to is out of range".to_string()))?;
    if (dates.end() - dates.start()).whole_days() > MAX_ADOPTION_DAYS {
        return Err(AiBillingError::Invalid(format!(
            "adoption covers at most {MAX_ADOPTION_DAYS} days"
        )));
    }
    Ok(dates)
}

/// The first day of the month `months` before `day`'s month.
fn months_before(day: Date, months: u8) -> Result<Date, AiBillingError> {
    let index = day.year() * 12 + i32::from(u8::from(day.month())) - 1 - i32::from(months);
    let month = Month::try_from((index.rem_euclid(12) + 1) as u8)
        .map_err(|_| AiBillingError::Invalid("from is out of range".to_string()))?;
    Date::from_calendar_date(index.div_euclid(12), month, 1)
        .map_err(|_| AiBillingError::Invalid("from is out of range".to_string()))
}

/// Each person's days per tool: their usage without completions, credited
/// buckets without their estimate, and their credits on the payer's tool. A
/// day of credits alone is a day of use too.
pub fn adoption_days(
    rows: Vec<AiAdoptionUsageRow>,
    credits: &[AiAdoptionCredit],
    payer: Option<AiProvider>,
) -> Result<Vec<AiAdoptionDay>, AiBillingError> {
    let mut days: HashMap<(UserId, Date, AiProvider), AiBillingUsage> = HashMap::new();
    for row in rows {
        if row.is_completion() {
            continue;
        }
        let usage = row.measured_usage();
        days.entry((row.user_id, row.day, row.provider))
            .or_default()
            .add(&usage)?;
    }
    if let Some(payer) = payer {
        // Rows of zero credits are kept by the import, but are no use.
        for credit in credits.iter().filter(|credit| credit.amount.nanos() > 0) {
            days.entry((credit.user_id, credit.day, payer))
                .or_default()
                .add(&AiBillingUsage {
                    priced_cost: credit.amount,
                    ..AiBillingUsage::default()
                })?;
        }
    }
    let mut days: Vec<AiAdoptionDay> = days
        .into_iter()
        .map(|((user_id, day, tool), usage)| AiAdoptionDay {
            user_id,
            day,
            tool,
            usage,
        })
        .collect();
    days.sort_by_key(|day| (day.user_id.as_i32(), day.day, day.tool));
    Ok(days)
}

/// What the company's credits on each UTC day hold beyond its people's: the
/// payer's total for the day less what its people were charged, never below
/// zero while one import lags the other.
pub fn company_days(
    totals: &[(Date, AiUsdNanos)],
    credits: &[AiAdoptionCredit],
    payer: AiProvider,
) -> Vec<AiAdoptionCompanyDay> {
    let mut people: HashMap<Date, i64> = HashMap::new();
    for credit in credits {
        *people.entry(credit.day).or_default() += credit.amount.nanos();
    }
    totals
        .iter()
        .filter_map(|(day, total)| {
            let rest = total.nanos() - people.get(day).copied().unwrap_or(0);
            (rest > 0).then(|| AiAdoptionCompanyDay {
                day: *day,
                tool: payer,
                amount: AiUsdNanos::new(rest).unwrap_or_default(),
            })
        })
        .collect()
}

/// Who adoption lists: everyone linked to time tracking, and anyone else with
/// a day of use.
pub fn listed_people(
    people: Vec<AiAdoptionPerson>,
    days: &[AiAdoptionDay],
) -> Vec<AiAdoptionPerson> {
    let users: HashSet<UserId> = days.iter().map(|day| day.user_id).collect();
    people
        .into_iter()
        .filter(|person| person.linked || users.contains(&person.user_id))
        .collect()
}

/// One person's models, largest first by records, completions left out, and
/// credited usage without its estimate.
pub fn adoption_models(
    rows: Vec<AiAdoptionUsageRow>,
) -> Result<Vec<AiAdoptionModel>, AiBillingError> {
    let mut models: BTreeMap<(AiProvider, String), AiBillingUsage> = BTreeMap::new();
    for row in rows {
        if row.is_completion() {
            continue;
        }
        let usage = row.measured_usage();
        models
            .entry((row.provider, row.model))
            .or_default()
            .add(&usage)?;
    }
    let mut models: Vec<AiAdoptionModel> = models
        .into_iter()
        .map(|((tool, model), usage)| AiAdoptionModel { tool, model, usage })
        .collect();
    models.sort_by_key(|model| std::cmp::Reverse(model.usage.records));
    Ok(models)
}

#[cfg(test)]
mod tests {
    use time::macros::date;

    use super::*;
    use crate::domain::models::AiTokenCounts;

    fn usage(records: i64, cost_nanos: i64) -> AiBillingUsage {
        AiBillingUsage {
            tokens: AiTokenCounts {
                input: records,
                cache_read: 10 * records,
                cache_write: 0,
                output: records,
            },
            records,
            priced_cost: AiUsdNanos::new(cost_nanos).unwrap(),
            unpriced_records: 0,
        }
    }

    fn row(
        user: i32,
        provider: AiProvider,
        billing_provider: Option<AiProvider>,
        model: &str,
        credited: bool,
        usage: AiBillingUsage,
    ) -> AiAdoptionUsageRow {
        AiAdoptionUsageRow {
            user_id: UserId::new(user),
            day: date!(2026 - 10 - 05),
            provider,
            billing_provider,
            model: model.to_string(),
            credited,
            usage,
        }
    }

    #[test]
    fn completions_are_not_usage_but_paid_models_are() {
        let days = adoption_days(
            vec![
                row(
                    1,
                    AiProvider::Copilot,
                    None,
                    "copilot-nes-x",
                    false,
                    usage(9, 0),
                ),
                row(
                    1,
                    AiProvider::Copilot,
                    None,
                    "gpt-4o-mini-2024-07-18",
                    false,
                    usage(4, 0),
                ),
                row(
                    1,
                    AiProvider::Copilot,
                    None,
                    "gpt-6.1-sol",
                    false,
                    usage(2, 50),
                ),
                // Not Copilot's to give away: another payer's model of that name.
                row(
                    2,
                    AiProvider::Codex,
                    None,
                    "gpt-4o-mini",
                    false,
                    usage(3, 7),
                ),
            ],
            &[],
            Some(AiProvider::Copilot),
        )
        .unwrap();

        assert_eq!(days.len(), 2);
        assert_eq!(days[0].usage, usage(2, 50));
        assert_eq!(days[1].tool, AiProvider::Codex);
        assert_eq!(days[1].usage, usage(3, 7));
    }

    #[test]
    fn credits_replace_the_estimate_of_the_usage_they_measure() {
        let days = adoption_days(
            vec![
                row(
                    1,
                    AiProvider::Copilot,
                    None,
                    "gpt-6.1-sol",
                    true,
                    usage(2, 50),
                ),
                row(
                    1,
                    AiProvider::Copilot,
                    None,
                    "gpt-6.1-sol",
                    false,
                    usage(1, 30),
                ),
                row(
                    1,
                    AiProvider::OpenCode,
                    Some(AiProvider::Copilot),
                    "gpt-6.1-sol",
                    true,
                    usage(5, 90),
                ),
            ],
            &[AiAdoptionCredit {
                user_id: UserId::new(1),
                day: date!(2026 - 10 - 05),
                amount: AiUsdNanos::new(400).unwrap(),
            }],
            Some(AiProvider::Copilot),
        )
        .unwrap();

        // The estimate before the credits' first instant stays; credits add.
        assert_eq!(days[0].tool, AiProvider::Copilot);
        assert_eq!(days[0].usage.records, 3);
        assert_eq!(days[0].usage.priced_cost.nanos(), 430);
        // OpenCode routed through Copilot keeps its tokens, not its estimate.
        assert_eq!(days[1].tool, AiProvider::OpenCode);
        assert_eq!(days[1].usage.records, 5);
        assert_eq!(days[1].usage.priced_cost, AiUsdNanos::ZERO);
    }

    #[test]
    fn a_day_of_credits_alone_is_a_day_of_use() {
        let days = adoption_days(
            Vec::new(),
            &[AiAdoptionCredit {
                user_id: UserId::new(3),
                day: date!(2026 - 10 - 06),
                amount: AiUsdNanos::new(25).unwrap(),
            }],
            Some(AiProvider::Copilot),
        )
        .unwrap();

        assert_eq!(days.len(), 1);
        assert_eq!(days[0].usage.records, 0);
        assert_eq!(days[0].usage.priced_cost.nanos(), 25);
    }

    #[test]
    fn zero_credits_are_no_day_of_use() {
        let days = adoption_days(
            vec![row(
                1,
                AiProvider::Copilot,
                None,
                "copilot-nes-x",
                false,
                usage(9, 0),
            )],
            &[AiAdoptionCredit {
                user_id: UserId::new(1),
                day: date!(2026 - 10 - 05),
                amount: AiUsdNanos::ZERO,
            }],
            Some(AiProvider::Copilot),
        )
        .unwrap();

        assert!(days.is_empty());
    }

    #[test]
    fn the_company_keeps_what_its_people_were_not_charged() {
        let credit = |user, day, amount| AiAdoptionCredit {
            user_id: UserId::new(user),
            day,
            amount: AiUsdNanos::new(amount).unwrap(),
        };
        let totals = [
            (date!(2026 - 10 - 01), AiUsdNanos::new(100).unwrap()),
            (date!(2026 - 10 - 02), AiUsdNanos::new(30).unwrap()),
            (date!(2026 - 10 - 03), AiUsdNanos::new(10).unwrap()),
        ];
        let rest = company_days(
            &totals,
            &[
                credit(1, date!(2026 - 10 - 01), 60),
                credit(2, date!(2026 - 10 - 01), 15),
                credit(1, date!(2026 - 10 - 02), 30),
                // The day's total is not imported yet: nothing is left over.
                credit(1, date!(2026 - 10 - 03), 40),
            ],
            AiProvider::Copilot,
        );

        assert_eq!(
            rest,
            vec![AiAdoptionCompanyDay {
                day: date!(2026 - 10 - 01),
                tool: AiProvider::Copilot,
                amount: AiUsdNanos::new(25).unwrap(),
            }]
        );
    }

    #[test]
    fn dates_default_to_six_months_through_today_and_are_bounded() {
        let today = date!(2026 - 03 - 15);
        let dates = adoption_dates(today, None, None).unwrap();
        assert_eq!(dates.start(), date!(2025 - 09 - 01));
        assert_eq!(dates.last_day(), today);

        assert!(adoption_dates(today, Some(date!(2026 - 03 - 16)), None).is_err());
        assert!(adoption_dates(today, Some(date!(2025 - 01 - 01)), None).is_err());
    }
}

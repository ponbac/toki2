//! AI subscriptions that developers declare, such as Claude Max or ChatGPT Pro.
//!
//! A subscription has a fixed monthly fee in its own currency and a period of
//! local calendar dates in the `ai_usage.time_zone` setting. Usage of a provider on
//! a local day that none of the user's subscriptions to that provider covers is
//! billed as API usage at its estimated cost. One user's subscriptions to one
//! provider never overlap, so at most one covers a day.
//!
//! Plan hints that token-ledger uploads, such as Codex `plan_type`, are evidence
//! only: declared subscriptions decide the billing mode.
//!
//! Date conventions: a subscription period ends *inclusively* on `valid_to`,
//! the way people state it, while `AiUsageDateRange` ends *exclusively*, the
//! way usage is summed. Convert with `AiSubscriptionPeriod::to_date_range` and
//! `AiUsageDateRange::from_inclusive` rather than by hand.

use std::fmt;

use time::{Date, Duration};

use super::{AiProvider, AiSubscriptionId, AiUsageDateRange, AiUsageTimeZone, UserId};
use crate::domain::AiSubscriptionError;

/// The longest plan name, in characters (Unicode code points).
pub const MAX_PLAN_CHARS: usize = 64;
/// Hinted plans that denote no paid plan, such as Codex `free`, in lower case
/// and trimmed. A hint of one of these, in any letter case, is no evidence of a
/// subscription, so it never flags a mismatch.
pub const UNPAID_PLAN_HINTS: &[&str] = &["free"];
/// How many local days, ending today, a mismatch search covers by default.
pub const DEFAULT_MISMATCH_DAYS: i64 = 90;
/// The most integer digits a monthly cost may have, as `NUMERIC(12, 2)` allows.
const MAX_COST_INTEGER_DIGITS: usize = 10;

/// A trimmed, free-text plan name, such as `Claude Max 5x` or `ChatGPT Pro`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSubscriptionPlan(String);

impl AiSubscriptionPlan {
    pub fn parse(raw: &str) -> Result<Self, AiSubscriptionError> {
        let plan = raw.trim();
        if plan.is_empty()
            || plan.chars().count() > MAX_PLAN_CHARS
            || plan.chars().any(char::is_control)
        {
            return Err(invalid(format!(
                "plan must contain 1 to {MAX_PLAN_CHARS} characters without control characters"
            )));
        }

        Ok(Self(plan.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// An exact, non-negative monthly fee with at most two decimals, such as `199.00`.
/// It is held as hundredths of the currency unit, so arithmetic stays exact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct AiMonthlyCost(i64);

impl AiMonthlyCost {
    /// Parses a plain decimal such as `2290`, `19.9` or `19.99`: no sign,
    /// exponent, or digit separators.
    pub fn parse(raw: &str) -> Result<Self, AiSubscriptionError> {
        let problem = || {
            invalid(format!(
                "monthlyCost must be a non-negative amount with at most two decimals and \
                 {MAX_COST_INTEGER_DIGITS} integer digits, such as 199.00"
            ))
        };
        let (units, fraction) = raw.trim().split_once('.').unwrap_or((raw.trim(), "00"));
        let digits = |text: &str, max: usize| {
            (1..=max).contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_digit())
        };
        if !digits(units, MAX_COST_INTEGER_DIGITS) || !digits(fraction, 2) {
            return Err(problem());
        }

        let units = units.parse::<i64>().map_err(|_| problem())?;
        let fraction = format!("{fraction:0<2}")
            .parse::<i64>()
            .map_err(|_| problem())?;
        Ok(Self(units * 100 + fraction))
    }

    /// A cost of `hundredths / 100` currency units, if it is not negative.
    pub fn from_hundredths(hundredths: i64) -> Option<Self> {
        (hundredths >= 0).then_some(Self(hundredths))
    }

    pub fn hundredths(self) -> i64 {
        self.0
    }
}

impl fmt::Display for AiMonthlyCost {
    /// Always two decimals, such as `2290.00`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{:02}", self.0 / 100, self.0 % 100)
    }
}

/// An ISO 4217 currency code in upper case, such as `SEK` or `USD`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AiCurrency(String);

impl AiCurrency {
    /// Accepts three ASCII letters in either case.
    pub fn parse(raw: &str) -> Result<Self, AiSubscriptionError> {
        let code = raw.trim();
        if code.len() != 3 || !code.bytes().all(|byte| byte.is_ascii_alphabetic()) {
            return Err(invalid("currency must be a three-letter code, such as SEK"));
        }

        Ok(Self(code.to_ascii_uppercase()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The positive, four-digit-year local dates a subscription covers, representable
/// as `YYYY-MM-DD` throughout the HTTP contract. Both ends are inclusive: a
/// subscription with `valid_to` 2026-01-31 covers 31 January. No end means the
/// subscription is ongoing. See `to_date_range` for the half-open form.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiSubscriptionPeriod {
    valid_from: Date,
    valid_to: Option<Date>,
}

impl AiSubscriptionPeriod {
    pub fn new(valid_from: Date, valid_to: Option<Date>) -> Result<Self, AiSubscriptionError> {
        let valid_year = |day: Date| (1..=9999).contains(&day.year());
        if !valid_year(valid_from) || valid_to.is_some_and(|day| !valid_year(day)) {
            return Err(invalid(
                "subscription dates must have a year from 0001 through 9999",
            ));
        }

        if valid_to.is_some_and(|valid_to| valid_to < valid_from) {
            return Err(invalid("validTo must not be before validFrom"));
        }

        Ok(Self {
            valid_from,
            valid_to,
        })
    }

    /// The first covered day.
    pub fn valid_from(&self) -> Date {
        self.valid_from
    }

    /// The last covered day, or `None` while the subscription is ongoing.
    pub fn valid_to(&self) -> Option<Date> {
        self.valid_to
    }

    pub fn contains(&self, day: Date) -> bool {
        self.valid_from <= day && self.valid_to.is_none_or(|valid_to| day <= valid_to)
    }

    /// The days of `within` that this period covers, as a half-open range whose
    /// end is the day *after* the last covered day, or `None` when it covers
    /// none of them. For example, a subscription through 15 March covers
    /// `[1 March, 16 March)` of the March range `[1 March, 1 April)`: 15 days.
    #[allow(
        dead_code,
        reason = "billing (#148) clips subscriptions to billing months with this"
    )]
    pub fn to_date_range(self, within: AiUsageDateRange) -> Option<AiUsageDateRange> {
        let start = self.valid_from.max(within.start());
        let end = match self.valid_to {
            // `valid_to` is before `within.end()`, so the next day exists.
            Some(valid_to) if valid_to < within.end() => valid_to.next_day()?,
            _ => within.end(),
        };

        AiUsageDateRange::new(start, end)
    }
}

/// What a developer declares about a subscription; everything but its identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSubscriptionTerms {
    pub provider: AiProvider,
    pub plan: AiSubscriptionPlan,
    pub monthly_cost: AiMonthlyCost,
    pub currency: AiCurrency,
    pub period: AiSubscriptionPeriod,
}

/// A stored subscription of one user to one provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSubscription {
    pub id: AiSubscriptionId,
    pub user_id: UserId,
    pub terms: AiSubscriptionTerms,
}

impl AiSubscription {
    /// Whether this subscription pays for `user_id`'s usage of `provider` on the
    /// local date `day`.
    pub fn covers(&self, user_id: UserId, provider: AiProvider, day: Date) -> bool {
        self.user_id == user_id
            && self.terms.provider == provider
            && self.terms.period.contains(day)
    }
}

/// Whose subscriptions an operation reads or changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiSubscriptionScope {
    /// One user's only: a developer's own, or an admin's filter.
    User(UserId),
    /// Every user's. Only admins act in this scope.
    AllUsers,
}

impl AiSubscriptionScope {
    /// The user the scope is limited to, if it is.
    pub fn user_id(self) -> Option<UserId> {
        match self {
            Self::User(user_id) => Some(user_id),
            Self::AllUsers => None,
        }
    }
}

/// How a user's usage of a provider on one local day is billed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiBillingMode<'a> {
    /// No subscription covers the day: usage is billed at its estimated API cost.
    Api,
    /// The subscription whose monthly fee pays for the day's usage.
    Subscription(&'a AiSubscription),
}

/// Resolves how `user_id`'s usage of `provider` on the local date `day` is billed.
///
/// `subscriptions` may hold any users and providers; only those of `user_id` for
/// `provider` count. A subscription covers every day from `valid_from` through
/// `valid_to` inclusive, or on without end when it is ongoing. Stored subscriptions
/// of one user to one provider never overlap, so at most one covers a day; should
/// the slice break that rule, the first covering subscription wins.
#[allow(
    dead_code,
    reason = "billing (#148) resolves each usage day with this; tests pin the mismatch query to it"
)]
pub fn resolve_billing_mode(
    subscriptions: &[AiSubscription],
    user_id: UserId,
    provider: AiProvider,
    day: Date,
) -> AiBillingMode<'_> {
    subscriptions
        .iter()
        .find(|subscription| subscription.covers(user_id, provider, day))
        .map_or(AiBillingMode::Api, AiBillingMode::Subscription)
}

/// A local day on which a provider reported a paid plan for a user, but no
/// declared subscription to that provider covers the day, so its usage bills as
/// API usage. Hints are evidence only: the day is flagged for review, and its
/// billing mode does not change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiPlanMismatchDay {
    pub user_id: UserId,
    pub provider: AiProvider,
    pub day: Date,
    /// Distinct paid plans reported that day, such as `pro`, in ascending order.
    pub plans: Vec<String>,
    /// The first day of the user's next subscription to the provider, if any.
    pub next_subscription_from: Option<Date>,
}

/// Mismatch days of one user and provider that no declared subscription
/// separates: they lie in one gap between subscriptions, so one declaration
/// can cover them all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiPlanMismatchRun {
    pub user_id: UserId,
    pub provider: AiProvider,
    /// The first mismatch day.
    pub first_day: Date,
    /// The last mismatch day, inclusive.
    pub last_day: Date,
    /// How many days in the run have mismatches; days without hints between
    /// them are not counted.
    pub days: u32,
    /// Distinct paid plans reported during the run, in ascending order.
    pub plans: Vec<String>,
    /// The last day before the provider's next declared subscription, or `None`
    /// when none follows. A subscription from `first_day` through this day,
    /// inclusive, overlaps none of the user's others.
    pub last_uncovered_day: Option<Date>,
}

/// Groups mismatch days, ordered by user, provider and day, into runs: a run
/// ends where a declared subscription of that user to that provider starts.
pub fn group_mismatch_runs(days: Vec<AiPlanMismatchDay>) -> Vec<AiPlanMismatchRun> {
    let mut runs: Vec<AiPlanMismatchRun> = Vec::new();
    for day in days {
        let last_uncovered_day = day.next_subscription_from.and_then(Date::previous_day);
        match runs.last_mut() {
            Some(run)
                if run.user_id == day.user_id
                    && run.provider == day.provider
                    && run.last_uncovered_day == last_uncovered_day =>
            {
                run.last_day = day.day;
                run.days += 1;
                run.plans.extend(day.plans);
                run.plans.sort();
                run.plans.dedup();
            }
            _ => runs.push(AiPlanMismatchRun {
                user_id: day.user_id,
                provider: day.provider,
                first_day: day.day,
                last_day: day.day,
                days: 1,
                plans: day.plans,
                last_uncovered_day,
            }),
        }
    }

    runs
}

/// The mismatch runs found in a range of local days.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiPlanMismatches {
    /// The local days searched.
    pub dates: AiUsageDateRange,
    pub runs: Vec<AiPlanMismatchRun>,
}

/// The range a mismatch search covers without explicit bounds: the
/// `DEFAULT_MISMATCH_DAYS` local days that end with `last_day`, inclusive.
pub fn default_mismatch_dates(last_day: Date) -> Option<AiUsageDateRange> {
    let first_day = last_day.checked_sub(Duration::days(DEFAULT_MISMATCH_DAYS - 1))?;
    AiUsageDateRange::from_inclusive(first_day, last_day)
}

/// The time zone whose calendar days subscription dates are, and today there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiLocalCalendar {
    pub time_zone: AiUsageTimeZone,
    pub today: Date,
}

fn invalid(message: impl Into<String>) -> AiSubscriptionError {
    AiSubscriptionError::Invalid(message.into())
}

#[cfg(test)]
mod tests {
    use time::macros::date;

    use super::*;

    const DEV: UserId = UserId::new(7);
    const OTHER_DEV: UserId = UserId::new(8);

    fn subscription(
        id: i32,
        user_id: UserId,
        provider: AiProvider,
        valid_from: Date,
        valid_to: Option<Date>,
    ) -> AiSubscription {
        AiSubscription {
            id: AiSubscriptionId::new(id),
            user_id,
            terms: AiSubscriptionTerms {
                provider,
                plan: AiSubscriptionPlan::parse("ChatGPT Pro").unwrap(),
                monthly_cost: AiMonthlyCost::parse("2290").unwrap(),
                currency: AiCurrency::parse("SEK").unwrap(),
                period: AiSubscriptionPeriod::new(valid_from, valid_to).unwrap(),
            },
        }
    }

    /// The id of the subscription that bills the day, or `None` for API billing.
    fn billed_by(subscriptions: &[AiSubscription], provider: AiProvider, day: Date) -> Option<i32> {
        match resolve_billing_mode(subscriptions, DEV, provider, day) {
            AiBillingMode::Api => None,
            AiBillingMode::Subscription(subscription) => Some(subscription.id.as_i32()),
        }
    }

    #[test]
    fn a_closed_period_covers_both_edges_and_nothing_outside() {
        let subscriptions = [subscription(
            1,
            DEV,
            AiProvider::Codex,
            date!(2026 - 03 - 10),
            Some(date!(2026 - 04 - 20)),
        )];

        for (day, expected) in [
            (date!(2026 - 03 - 09), None),
            (date!(2026 - 03 - 10), Some(1)),
            (date!(2026 - 04 - 20), Some(1)),
            (date!(2026 - 04 - 21), None),
        ] {
            assert_eq!(
                billed_by(&subscriptions, AiProvider::Codex, day),
                expected,
                "{day}"
            );
        }
    }

    #[test]
    fn a_one_day_period_covers_only_that_day() {
        let day = date!(2026 - 05 - 15);
        let subscriptions = [subscription(1, DEV, AiProvider::Claude, day, Some(day))];

        assert_eq!(billed_by(&subscriptions, AiProvider::Claude, day), Some(1));
        assert_eq!(
            billed_by(
                &subscriptions,
                AiProvider::Claude,
                day.previous_day().unwrap()
            ),
            None
        );
        assert_eq!(
            billed_by(&subscriptions, AiProvider::Claude, day.next_day().unwrap()),
            None
        );
    }

    #[test]
    fn an_ongoing_subscription_covers_every_day_from_its_start() {
        let subscriptions = [subscription(
            1,
            DEV,
            AiProvider::Claude,
            date!(2026 - 01 - 01),
            None,
        )];

        assert_eq!(
            billed_by(&subscriptions, AiProvider::Claude, date!(2025 - 12 - 31)),
            None
        );
        for day in [
            date!(2026 - 01 - 01),
            date!(2026 - 12 - 31),
            date!(2031 - 06 - 30),
        ] {
            assert_eq!(
                billed_by(&subscriptions, AiProvider::Claude, day),
                Some(1),
                "{day}"
            );
        }
    }

    #[test]
    fn consecutive_subscriptions_hand_over_at_month_boundaries() {
        // Plus through January, Pro from February, and API billing in the gap
        // left by ending Pro on the last day of a leap-year February.
        let subscriptions = [
            subscription(
                1,
                DEV,
                AiProvider::Codex,
                date!(2027 - 12 - 01),
                Some(date!(2028 - 01 - 31)),
            ),
            subscription(
                2,
                DEV,
                AiProvider::Codex,
                date!(2028 - 02 - 01),
                Some(date!(2028 - 02 - 29)),
            ),
            subscription(3, DEV, AiProvider::Codex, date!(2028 - 04 - 01), None),
        ];

        for (day, expected) in [
            (date!(2027 - 11 - 30), None),
            (date!(2027 - 12 - 01), Some(1)),
            (date!(2027 - 12 - 31), Some(1)),
            (date!(2028 - 01 - 01), Some(1)),
            (date!(2028 - 01 - 31), Some(1)),
            (date!(2028 - 02 - 01), Some(2)),
            (date!(2028 - 02 - 29), Some(2)),
            (date!(2028 - 03 - 01), None),
            (date!(2028 - 03 - 31), None),
            (date!(2028 - 04 - 01), Some(3)),
        ] {
            assert_eq!(
                billed_by(&subscriptions, AiProvider::Codex, day),
                expected,
                "{day}"
            );
        }
    }

    #[test]
    fn only_the_users_own_subscription_to_that_provider_counts() {
        let day = date!(2026 - 09 - 22);
        let subscriptions = [
            subscription(1, OTHER_DEV, AiProvider::Codex, date!(2026 - 01 - 01), None),
            subscription(2, DEV, AiProvider::Claude, date!(2026 - 01 - 01), None),
        ];

        assert_eq!(billed_by(&subscriptions, AiProvider::Codex, day), None);
        assert_eq!(billed_by(&subscriptions, AiProvider::Claude, day), Some(2));
        assert_eq!(billed_by(&[], AiProvider::Claude, day), None);
    }

    #[test]
    fn unpaid_plan_hints_are_lower_case_and_trimmed_for_matching() {
        assert!(UNPAID_PLAN_HINTS.contains(&"free"));
        for plan in UNPAID_PLAN_HINTS {
            assert_eq!(*plan, plan.trim().to_lowercase(), "{plan:?}");
        }
    }

    fn mismatch_day(
        user_id: UserId,
        provider: AiProvider,
        day: Date,
        plan: &str,
        next_subscription_from: Option<Date>,
    ) -> AiPlanMismatchDay {
        AiPlanMismatchDay {
            user_id,
            provider,
            day,
            plans: vec![plan.to_string()],
            next_subscription_from,
        }
    }

    #[test]
    fn mismatch_days_between_the_same_subscriptions_form_one_run() {
        let september = Some(date!(2026 - 09 - 01));
        let runs = group_mismatch_runs(vec![
            // Two days in the gap before a subscription that starts in September,
            // with a hint-free weekend between them.
            mismatch_day(
                DEV,
                AiProvider::Codex,
                date!(2026 - 08 - 07),
                "pro",
                september,
            ),
            mismatch_day(
                DEV,
                AiProvider::Codex,
                date!(2026 - 08 - 10),
                "plus",
                september,
            ),
            // The gap after it, with no later subscription.
            mismatch_day(DEV, AiProvider::Codex, date!(2026 - 10 - 02), "pro", None),
            mismatch_day(DEV, AiProvider::Grok, date!(2026 - 10 - 02), "heavy", None),
            mismatch_day(
                OTHER_DEV,
                AiProvider::Grok,
                date!(2026 - 10 - 03),
                "heavy",
                None,
            ),
        ]);

        let run =
            |user_id, provider, first_day, last_day, days, plans: &[&str], last_uncovered_day| {
                AiPlanMismatchRun {
                    user_id,
                    provider,
                    first_day,
                    last_day,
                    days,
                    plans: plans.iter().map(ToString::to_string).collect(),
                    last_uncovered_day,
                }
            };
        assert_eq!(
            runs,
            [
                run(
                    DEV,
                    AiProvider::Codex,
                    date!(2026 - 08 - 07),
                    date!(2026 - 08 - 10),
                    2,
                    &["plus", "pro"],
                    Some(date!(2026 - 08 - 31)),
                ),
                run(
                    DEV,
                    AiProvider::Codex,
                    date!(2026 - 10 - 02),
                    date!(2026 - 10 - 02),
                    1,
                    &["pro"],
                    None,
                ),
                run(
                    DEV,
                    AiProvider::Grok,
                    date!(2026 - 10 - 02),
                    date!(2026 - 10 - 02),
                    1,
                    &["heavy"],
                    None,
                ),
                run(
                    OTHER_DEV,
                    AiProvider::Grok,
                    date!(2026 - 10 - 03),
                    date!(2026 - 10 - 03),
                    1,
                    &["heavy"],
                    None,
                ),
            ]
        );
        assert!(group_mismatch_runs(Vec::new()).is_empty());
    }

    #[test]
    fn a_period_converts_to_the_half_open_range_it_covers() {
        let march = AiUsageDateRange::new(date!(2026 - 03 - 01), date!(2026 - 04 - 01)).unwrap();
        let covered = |valid_from, valid_to| {
            AiSubscriptionPeriod::new(valid_from, valid_to)
                .unwrap()
                .to_date_range(march)
                .map(|range| (range.start(), range.end()))
        };

        for (valid_from, valid_to, expected) in [
            // The inclusive last day becomes the exclusive next day.
            (
                date!(2026 - 03 - 10),
                Some(date!(2026 - 03 - 15)),
                Some((date!(2026 - 03 - 10), date!(2026 - 03 - 16))),
            ),
            (
                date!(2026 - 03 - 15),
                Some(date!(2026 - 03 - 15)),
                Some((date!(2026 - 03 - 15), date!(2026 - 03 - 16))),
            ),
            // Ending on the month's last day covers the whole rest of the month.
            (
                date!(2026 - 02 - 01),
                Some(date!(2026 - 03 - 31)),
                Some((date!(2026 - 03 - 01), date!(2026 - 04 - 01))),
            ),
            (
                date!(2026 - 03 - 20),
                None,
                Some((date!(2026 - 03 - 20), date!(2026 - 04 - 01))),
            ),
            (date!(2026 - 01 - 01), Some(date!(2026 - 02 - 28)), None),
            (date!(2026 - 04 - 01), None, None),
        ] {
            assert_eq!(
                covered(valid_from, valid_to),
                expected,
                "{valid_from}..={valid_to:?}"
            );
        }

        let whole_range = AiUsageDateRange::new(Date::MIN, Date::MAX).unwrap();
        let ongoing = AiSubscriptionPeriod::new(date!(2026 - 01 - 01), None).unwrap();
        assert_eq!(
            ongoing.to_date_range(whole_range).map(|range| range.end()),
            Some(Date::MAX)
        );
    }

    #[test]
    fn the_default_mismatch_search_is_the_last_ninety_days() {
        let dates = default_mismatch_dates(date!(2026 - 09 - 28)).unwrap();
        assert_eq!(
            (dates.start(), dates.last_day(), dates.end()),
            (
                date!(2026 - 07 - 01),
                date!(2026 - 09 - 28),
                date!(2026 - 09 - 29)
            )
        );
    }

    #[test]
    fn monthly_costs_are_exact_decimals_with_at_most_two_places() {
        for (raw, hundredths, shown) in [
            ("2290", 229_000, "2290.00"),
            ("19.9", 1_990, "19.90"),
            ("19.99", 1_999, "19.99"),
            (" 0.05 ", 5, "0.05"),
            ("0", 0, "0.00"),
            ("9999999999.99", 999_999_999_999, "9999999999.99"),
        ] {
            let cost = AiMonthlyCost::parse(raw).unwrap();
            assert_eq!(cost.hundredths(), hundredths, "{raw:?}");
            assert_eq!(cost.to_string(), shown, "{raw:?}");
        }

        for raw in [
            "",
            ".",
            "1.",
            ".5",
            "-1",
            "+1",
            "1.999",
            "1e3",
            "1,50",
            "1 000",
            "0x10",
            "NaN",
            "12345678901",
        ] {
            assert!(AiMonthlyCost::parse(raw).is_err(), "{raw:?}");
        }
        assert_eq!(AiMonthlyCost::from_hundredths(-1), None);
    }

    #[test]
    fn plans_and_currencies_are_normalised_and_bounded() {
        assert_eq!(
            AiSubscriptionPlan::parse("  Claude Max 5x ")
                .unwrap()
                .as_str(),
            "Claude Max 5x"
        );
        assert!(AiSubscriptionPlan::parse("   ").is_err());
        assert!(AiSubscriptionPlan::parse("Pro\nPlus").is_err());
        assert!(AiSubscriptionPlan::parse(&"é".repeat(MAX_PLAN_CHARS)).is_ok());
        assert!(AiSubscriptionPlan::parse(&"é".repeat(MAX_PLAN_CHARS + 1)).is_err());

        assert_eq!(AiCurrency::parse("sek").unwrap().as_str(), "SEK");
        for raw in ["", "SE", "SEKK", "S3K", "kr"] {
            assert!(AiCurrency::parse(raw).is_err(), "{raw:?}");
        }
    }

    #[test]
    fn subscription_dates_have_positive_four_digit_years() {
        for day in [date!(0000 - 01 - 01), date!(-0001 - 01 - 01)] {
            assert!(AiSubscriptionPeriod::new(day, None).is_err());
        }

        let last_day = date!(9999 - 12 - 31);
        assert!(AiSubscriptionPeriod::new(last_day, Some(last_day)).is_ok());
    }

    #[test]
    fn a_period_does_not_end_before_it_starts() {
        let day = date!(2026 - 09 - 22);
        assert!(AiSubscriptionPeriod::new(day, Some(day)).is_ok());
        assert!(AiSubscriptionPeriod::new(day, None).is_ok());
        assert!(AiSubscriptionPeriod::new(day, day.previous_day()).is_err());
    }
}

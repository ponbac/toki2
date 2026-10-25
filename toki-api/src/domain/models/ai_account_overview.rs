//! The admin overview of a metered payer's accounts: each account with its
//! seats, dispositions and suggestions, and the two lists that need a look.
//!
//! The review window runs from the first day of the previous UTC month through
//! today, so last month can still be settled early in this one.
//!
//! - **Accounts without a disposition** are accounts that held a seat on a
//!   window day that no disposition covers: none at all, one that starts late
//!   or has ended, or a gap. An account whose seat ended before the window is
//!   never flagged.
//! - **Users with metered usage but no link** are users whose usage the payer
//!   paid for on window days that none of their `User` dispositions covers.
//! - **Accounts with credits but no disposition** are accounts the usage
//!   import found credits for on window days that no disposition covers, seat
//!   or no seat: every account with credits needs a disposition.
//! - **The default period** of a new disposition covers the uncovered seat
//!   days (`default_period`); once those are covered, or for an account that
//!   never held a seat, the uncovered credit days (`credit_default_period`).

use std::collections::HashMap;

use time::{Date, Month};

use super::{
    day_ranges, namespace_noise, suggest_targets, AiAccountCreditDay, AiAccountDispositionId,
    AiAccountPeriod, AiAccountSuggestion, AiBillingDeveloper, AiBillingMonth, AiBillingUsage,
    AiDirectoryStatus, AiMeteredUserDay, AiProvider, AiProviderAccountId, AiStoredProviderAccount,
    AiSuggestionAccount, AiUsageDateRange, AiUsdNanos, AiUserActivity, UserId,
};
use crate::domain::AiProviderAccountError;

/// Whether the payer's account directory can be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiDirectoryHealth {
    Ready,
    /// No credentials are configured, so the directory is off.
    Disabled,
    /// Credentials are configured but unusable, for the given reason.
    Misconfigured(String),
}

/// The directory's state and its last refreshes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiDirectoryOverview {
    pub health: AiDirectoryHealth,
    /// Where the payer's accounts live, such as a GitHub organization.
    pub namespace: String,
    pub status: Option<AiDirectoryStatus>,
}

/// One account as the overview shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAccountRow {
    pub account: AiStoredProviderAccount,
    /// The disposition whose period holds today.
    pub current_disposition: Option<AiAccountDispositionId>,
    /// Seated days in the review window that no disposition covers.
    pub uncovered_seat_days: Vec<AiUsageDateRange>,
    /// The account's gross credits (`G`) in the current UTC month.
    pub credits_this_month: AiUsdNanos,
    /// Review window days with credits that no disposition covers.
    pub uncovered_credit_days: Vec<AiUsageDateRange>,
    /// Review window days with credits outside every seat.
    pub credit_days_without_seat: u32,
    /// Uncovered seat days or uncovered credit days.
    pub needs_look: bool,
    /// Best first; empty when none or when `default_period` is `None`.
    pub suggestions: Vec<AiAccountSuggestion>,
    /// The period a new disposition defaults to; see the module documentation.
    pub default_period: Option<AiAccountPeriod>,
}

/// A user whose metered usage in the review window has no link on some days.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiUnlinkedUser {
    pub user_id: UserId,
    /// Days with usage that none of the user's dispositions covers.
    pub uncovered_days: u32,
    pub first_day: Date,
    pub last_day: Date,
    /// The usage on those days.
    pub usage: AiBillingUsage,
    /// Accounts whose top suggestion is the user.
    pub suggested_accounts: Vec<AiProviderAccountId>,
}

/// The pure part of the overview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAccountOverview {
    pub today: Date,
    pub review_from: Date,
    /// Accounts needing a look first, then by their top candidate's activity,
    /// then by login in any case.
    pub accounts: Vec<AiAccountRow>,
    /// Each user's metered usage in the review window, by user id.
    pub activity: Vec<(UserId, AiUserActivity)>,
    /// By the cost of their unlinked usage, largest first.
    pub unlinked_users: Vec<AiUnlinkedUser>,
    /// The month of each account's `credits_this_month`.
    pub credits_month: AiBillingMonth,
}

/// Everything the admin page shows about a payer's accounts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProviderAccountOverview {
    pub payer: AiProvider,
    pub directory: AiDirectoryOverview,
    pub overview: AiAccountOverview,
}

/// What the overview is computed from.
#[derive(Debug, Clone)]
pub struct AiAccountOverviewInput {
    pub today: Date,
    pub review_from: Date,
    pub accounts: Vec<AiStoredProviderAccount>,
    pub users: Vec<AiBillingDeveloper>,
    /// The payer's usage per user and UTC day in the review window.
    pub metered: Vec<AiMeteredUserDay>,
    /// The namespace whose words logins carry (`namespace_noise`).
    pub namespace: String,
    /// Each account's gross credits per UTC day, from the usage import, in
    /// the review window and `credits_month`.
    pub credits: Vec<AiAccountCreditDay>,
    /// The month `credits_this_month` covers: today's UTC month.
    pub credits_month: AiBillingMonth,
}

/// The first day of the UTC month before `today`'s.
pub fn review_from(today: Date) -> Date {
    let (year, month) = match today.month() {
        Month::January => (today.year() - 1, Month::December),
        month => (today.year(), month.previous()),
    };
    Date::from_calendar_date(year, month, 1).unwrap_or(today)
}

/// The review window, `[review_from, today]`, as a half-open range.
pub fn review_window(today: Date) -> Option<AiUsageDateRange> {
    AiUsageDateRange::from_inclusive(review_from(today), today)
}

/// Each user's metered usage, by user id.
pub fn user_activity(
    metered: &[AiMeteredUserDay],
) -> Result<HashMap<UserId, AiUserActivity>, AiProviderAccountError> {
    let mut activity: HashMap<UserId, AiUserActivity> = HashMap::new();
    for day in metered {
        activity.entry(day.user_id).or_default().add(day)?;
    }
    Ok(activity)
}

/// The suggestions for every account, as `suggest_targets` makes them, for
/// the review window `window`, which ends today.
pub fn account_suggestions(
    accounts: &[AiStoredProviderAccount],
    users: &[AiBillingDeveloper],
    activity: &HashMap<UserId, AiUserActivity>,
    namespace: &str,
    window: AiUsageDateRange,
) -> HashMap<AiProviderAccountId, Vec<AiAccountSuggestion>> {
    suggestions_for(
        accounts,
        |account| account.default_period(window.last_day(), window),
        users,
        activity,
        namespace,
    )
}

/// The suggestions for every account, each for the period a new disposition
/// of it would get.
fn suggestions_for(
    accounts: &[AiStoredProviderAccount],
    default_period: impl Fn(&AiStoredProviderAccount) -> Option<AiAccountPeriod>,
    users: &[AiBillingDeveloper],
    activity: &HashMap<UserId, AiUserActivity>,
    namespace: &str,
) -> HashMap<AiProviderAccountId, Vec<AiAccountSuggestion>> {
    let suggestion_accounts: Vec<AiSuggestionAccount> = accounts
        .iter()
        .map(|account| AiSuggestionAccount {
            account_id: account.account_id,
            login: account.login.as_str().to_string(),
            name: account
                .profile
                .as_ref()
                .and_then(|profile| profile.name.clone()),
            email: account
                .profile
                .as_ref()
                .and_then(|profile| profile.email.clone()),
            default_period: default_period(account),
        })
        .collect();
    let dispositions: Vec<_> = accounts
        .iter()
        .flat_map(|account| account.dispositions.iter().cloned())
        .collect();
    suggest_targets(
        &suggestion_accounts,
        users,
        activity,
        &dispositions,
        &namespace_noise(namespace),
    )
}

/// Builds the overview; see the module documentation.
pub fn account_overview(
    input: AiAccountOverviewInput,
) -> Result<AiAccountOverview, AiProviderAccountError> {
    let AiAccountOverviewInput {
        today,
        review_from,
        accounts,
        users,
        metered,
        namespace,
        credits,
        credits_month,
    } = input;
    let window = AiUsageDateRange::from_inclusive(review_from, today).ok_or_else(|| {
        AiProviderAccountError::Invalid("the review window ends before it starts".to_string())
    })?;
    let metered: Vec<AiMeteredUserDay> = metered
        .into_iter()
        .filter(|day| window.start() <= day.day && day.day < window.end())
        .collect();
    let activity = user_activity(&metered)?;
    let mut credits_by_account: HashMap<AiProviderAccountId, Vec<AiAccountCreditDay>> =
        HashMap::new();
    for day in credits.into_iter().filter(|day| day.gross.nanos() > 0) {
        credits_by_account
            .entry(day.account_id)
            .or_default()
            .push(day);
    }
    let uncovered_credit_days = |account: &AiStoredProviderAccount| {
        day_ranges(
            credits_by_account
                .get(&account.account_id)
                .into_iter()
                .flatten()
                .map(|day| day.day)
                .filter(|day| window.start() <= *day && *day < window.end())
                .filter(|day| account.disposition_on(*day).is_none()),
        )
    };
    // The seat days' default, else the credit days' once every seat day is
    // covered (or the account never held a seat), so an account that needs a
    // look for its credits alone can be given a disposition too.
    let default_periods: HashMap<AiProviderAccountId, Option<AiAccountPeriod>> = accounts
        .iter()
        .map(|account| {
            let period = match account.default_period(today, window) {
                Some(period) if !account.seats.is_empty() => Some(period),
                seat_default => account
                    .credit_default_period(&uncovered_credit_days(account))
                    .or(seat_default),
            };
            (account.account_id, period)
        })
        .collect();
    let mut suggestions = suggestions_for(
        &accounts,
        |account| default_periods.get(&account.account_id).copied().flatten(),
        &users,
        &activity,
        &namespace,
    );

    let mut rows: Vec<AiAccountRow> = accounts
        .into_iter()
        .map(|account| {
            let uncovered_seat_days = account.uncovered_seat_days(today, window);
            let uncovered_credit_days = uncovered_credit_days(&account);
            let credits = credits_by_account
                .get(&account.account_id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            let credits_this_month = credits
                .iter()
                .filter(|day| credits_month.contains(day.day))
                .try_fold(0_i64, |total, day| total.checked_add(day.gross.nanos()))
                .and_then(AiUsdNanos::new)
                .ok_or_else(|| {
                    AiProviderAccountError::Storage(
                        "an account's credits exceed the supported range".to_string(),
                    )
                })?;
            let window_credit_days: Vec<Date> = credits
                .iter()
                .map(|day| day.day)
                .filter(|day| window.start() <= *day && *day < window.end())
                .collect();
            let seated = account.seated_days(today);
            let credit_days_without_seat = window_credit_days
                .iter()
                .filter(|day| {
                    !seated
                        .iter()
                        .any(|range| range.start() <= **day && **day < range.end())
                })
                .count();
            Ok(AiAccountRow {
                current_disposition: account
                    .disposition_on(today)
                    .map(|disposition| disposition.id),
                needs_look: !uncovered_seat_days.is_empty() || !uncovered_credit_days.is_empty(),
                uncovered_seat_days,
                credits_this_month,
                uncovered_credit_days,
                credit_days_without_seat: u32::try_from(credit_days_without_seat)
                    .unwrap_or(u32::MAX),
                suggestions: suggestions.remove(&account.account_id).unwrap_or_default(),
                default_period: default_periods.get(&account.account_id).copied().flatten(),
                account,
            })
        })
        .collect::<Result<_, AiProviderAccountError>>()?;
    let top_activity = |row: &AiAccountRow| {
        row.suggestions
            .first()
            .and_then(|top| activity.get(&top.user_id))
            .map(AiUserActivity::rank)
            .unwrap_or_default()
    };
    rows.sort_by_cached_key(|row| {
        (
            !row.needs_look,
            std::cmp::Reverse(top_activity(row)),
            row.account.login.as_str().to_lowercase(),
            row.account.account_id,
        )
    });

    let mut suggested: HashMap<UserId, Vec<AiProviderAccountId>> = HashMap::new();
    for row in &rows {
        if let Some(top) = row.suggestions.first() {
            suggested
                .entry(top.user_id)
                .or_default()
                .push(row.account.account_id);
        }
    }
    let user_periods = |user_id: UserId| {
        rows.iter()
            .flat_map(|row| row.account.dispositions.iter())
            .filter(move |disposition| disposition.terms.target.user_id() == Some(user_id))
            .map(|disposition| disposition.terms.period)
            .collect::<Vec<_>>()
    };

    let mut by_user: HashMap<UserId, Vec<&AiMeteredUserDay>> = HashMap::new();
    for day in &metered {
        by_user.entry(day.user_id).or_default().push(day);
    }
    let mut unlinked_users = Vec::new();
    for (user_id, days) in by_user {
        let periods = user_periods(user_id);
        let mut uncovered = AiUserActivity::default();
        let mut first_day: Option<Date> = None;
        for day in days
            .into_iter()
            .filter(|day| !periods.iter().any(|period| period.contains(day.day)))
        {
            uncovered.add(day)?;
            first_day = Some(first_day.map_or(day.day, |first| first.min(day.day)));
        }
        if let (Some(first_day), Some(last_day)) = (first_day, uncovered.last_day) {
            unlinked_users.push(AiUnlinkedUser {
                user_id,
                uncovered_days: uncovered.days,
                first_day,
                last_day,
                usage: uncovered.usage,
                suggested_accounts: suggested.remove(&user_id).unwrap_or_default(),
            });
        }
    }
    unlinked_users.sort_by_key(|user| {
        (
            std::cmp::Reverse(user.usage.priced_cost.nanos()),
            std::cmp::Reverse(user.usage.records),
            user.user_id.as_i32(),
        )
    });

    let mut activity: Vec<(UserId, AiUserActivity)> = activity.into_iter().collect();
    activity.sort_by_key(|(user_id, _)| user_id.as_i32());

    Ok(AiAccountOverview {
        today,
        review_from,
        accounts: rows,
        activity,
        unlinked_users,
        credits_month,
    })
}

#[cfg(test)]
mod tests;

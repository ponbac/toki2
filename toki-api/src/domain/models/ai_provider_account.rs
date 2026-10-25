//! Accounts at metered AI payers, their seats, and whose usage each account's
//! is.
//!
//! A metered payer bills usage per account: GitHub bills Copilot per seat and
//! per credit to the GitHub account that used it. Toki reads the accounts that
//! hold seats from the payer's account directory (`ProviderAccountDirectory`)
//! and keeps every account and seat it ever saw, so usage on a seat that has
//! since ended still has an account.
//!
//! - An account is identified by the payer's numeric id, which survives renames;
//!   its login is the latest one seen.
//! - A seat is one assignment, identified by the payer's own assignment instant.
//!   It ends only when a complete listing omits its account, or when its
//!   account is listed with another assignment.
//! - A disposition says whose an account's usage is on a range of UTC calendar
//!   days, GitHub's billing days: a Toki user's, or no Toki user's (External,
//!   Spinit overhead). An account without one on a day is unlinked that day.
//!   One account's dispositions never share a day, nor do one user's for one
//!   payer; the store enforces both.
//!
//! Date conventions are those of `AiSubscriptionPeriod`: a disposition ends
//! *inclusively* on `valid_to`, while `AiUsageDateRange` ends *exclusively*.

use std::fmt;

use time::{Date, Duration, OffsetDateTime, UtcOffset};

use super::{AiAccountDispositionId, AiBillingUsage, AiProvider, AiUsageDateRange, UserId};
use crate::domain::AiProviderAccountError;

/// The longest login, in characters.
pub const MAX_LOGIN_CHARS: usize = 100;
/// The longest stored profile name or email, in characters.
pub const MAX_PROFILE_CHARS: usize = 512;
/// The longest stored client of a seat's last activity, in characters.
pub const MAX_ACTIVITY_CLIENT_CHARS: usize = 512;
/// How long after a failed refresh attempt the next one is due, on a UTC day
/// without a successful refresh.
pub const DIRECTORY_RETRY_AFTER: Duration = Duration::minutes(55);

/// A payer's numeric account id, such as a GitHub user id. Positive, and kept
/// by the payer across renames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AiProviderAccountId(i64);

impl AiProviderAccountId {
    pub fn new(id: i64) -> Option<Self> {
        (id > 0).then_some(Self(id))
    }

    pub fn as_i64(self) -> i64 {
        self.0
    }
}

impl fmt::Display for AiProviderAccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// An account's login at its payer, such as a GitHub username: 1 to
/// `MAX_LOGIN_CHARS` characters without whitespace or control characters.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AiProviderLogin(String);

impl AiProviderLogin {
    pub fn parse(raw: &str) -> Result<Self, AiProviderAccountError> {
        let login = raw.trim();
        if login.is_empty()
            || login.chars().count() > MAX_LOGIN_CHARS
            || login
                .chars()
                .any(|ch| ch.is_whitespace() || ch.is_control())
        {
            return Err(invalid(format!(
                "a login must have 1 to {MAX_LOGIN_CHARS} characters without whitespace"
            )));
        }
        Ok(Self(login.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for AiProviderLogin {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// `raw` trimmed and cut to `max_chars` characters, or `None` when nothing is
/// left: how optional free text from a payer is stored.
pub fn clip_text(raw: Option<&str>, max_chars: usize) -> Option<String> {
    let text = raw?.trim();
    let clipped: String = text.chars().take(max_chars).collect();
    let clipped = clipped.trim_end();
    (!clipped.is_empty()).then(|| clipped.to_string())
}

/// The UTC calendar day of `instant`.
pub fn utc_day(instant: OffsetDateTime) -> Date {
    instant.to_offset(UtcOffset::UTC).date()
}

/// A seat as the directory lists it now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiListedSeat {
    pub account_id: AiProviderAccountId,
    pub login: AiProviderLogin,
    /// The payer's assignment instant. Compared as an instant, whatever offset
    /// the payer renders it in.
    pub created_at: OffsetDateTime,
    /// The day the payer will cancel the seat, if a cancellation is pending.
    pub pending_cancellation_on: Option<Date>,
    pub last_activity_at: Option<OffsetDateTime>,
    /// The client of the last activity, at most `MAX_ACTIVITY_CLIENT_CHARS`.
    pub last_activity_client: Option<String>,
}

/// Every seat a directory listed in one read, deduplicated by account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSeatListing {
    pub seats: Vec<AiListedSeat>,
    /// How many seats the payer reported, if it did.
    pub reported_total: Option<u32>,
    /// Whether the last page read was shorter than a full page, so no page
    /// follows.
    pub ended_on_short_page: bool,
}

impl AiSeatListing {
    /// Whether the listing holds every seat, so it may end the seats it omits:
    /// pagination ended on a short page and it holds at least as many accounts
    /// as the payer reported. Listing more than reported (seats pending
    /// cancellation, say) is complete; fewer is not.
    pub fn is_complete(&self) -> bool {
        self.ended_on_short_page
            && self
                .reported_total
                .is_some_and(|total| self.seats.len() >= total as usize)
    }
}

/// An account's public profile at its payer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiProviderAccountProfile {
    pub account_id: AiProviderAccountId,
    /// The current login, which follows renames.
    pub login: AiProviderLogin,
    pub name: Option<String>,
    /// The public email only.
    pub email: Option<String>,
}

/// A stored seat: one assignment, with when Toki saw it listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiStoredSeat {
    pub created_at: OffsetDateTime,
    pub pending_cancellation_on: Option<Date>,
    pub last_activity_at: Option<OffsetDateTime>,
    pub last_activity_client: Option<String>,
    pub first_listed_at: OffsetDateTime,
    pub last_listed_at: OffsetDateTime,
    /// The first complete listing without the seat; `None` while it is listed.
    pub unlisted_at: Option<OffsetDateTime>,
}

impl AiStoredSeat {
    /// The seat's first UTC day: its assignment's.
    pub fn first_day(&self) -> Date {
        utc_day(self.created_at)
    }

    /// The seat's last UTC day, inclusive; `None` while it is listed. Once
    /// unlisted, its pending cancellation day if that lies from the day it was
    /// last listed through the day it was found unlisted, and otherwise the
    /// day it was last listed. Never before `first_day`.
    pub fn last_day(&self) -> Option<Date> {
        let unlisted_on = utc_day(self.unlisted_at?);
        let last_listed_on = utc_day(self.last_listed_at);
        let last_day = match self.pending_cancellation_on {
            Some(day) if last_listed_on <= day && day <= unlisted_on => day,
            _ => last_listed_on,
        };
        Some(last_day.max(self.first_day()))
    }

    pub fn is_listed(&self) -> bool {
        self.unlisted_at.is_none()
    }
}

/// Whose an account's usage is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiAccountTarget {
    /// A Toki user's.
    User(UserId),
    /// No Toki user's: Spinit overhead. An explicit decision, unlike having no
    /// disposition (unlinked).
    External,
}

impl AiAccountTarget {
    pub fn user_id(self) -> Option<UserId> {
        match self {
            Self::User(user_id) => Some(user_id),
            Self::External => None,
        }
    }
}

/// The UTC calendar days a disposition covers, both ends inclusive, with years
/// 0001 through 9999. No end means it is ongoing. The rules of
/// `AiSubscriptionPeriod`, in UTC days (GitHub's billing days) and with this
/// domain's errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiAccountPeriod {
    valid_from: Date,
    valid_to: Option<Date>,
}

impl AiAccountPeriod {
    pub fn new(valid_from: Date, valid_to: Option<Date>) -> Result<Self, AiProviderAccountError> {
        let valid_year = |day: Date| (1..=9999).contains(&day.year());
        if !valid_year(valid_from) || valid_to.is_some_and(|day| !valid_year(day)) {
            return Err(invalid(
                "disposition dates must have a year from 0001 through 9999",
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

    /// The last covered day, or `None` while ongoing.
    pub fn valid_to(&self) -> Option<Date> {
        self.valid_to
    }

    pub fn contains(&self, day: Date) -> bool {
        self.valid_from <= day && self.valid_to.is_none_or(|valid_to| day <= valid_to)
    }

    /// Whether the two periods share a day.
    pub fn overlaps(&self, other: &Self) -> bool {
        self.valid_to.is_none_or(|to| other.valid_from <= to)
            && other.valid_to.is_none_or(|to| self.valid_from <= to)
    }

    /// The days of `within` that this period covers, as a half-open range, or
    /// `None` when it covers none of them.
    pub fn to_date_range(self, within: AiUsageDateRange) -> Option<AiUsageDateRange> {
        let start = self.valid_from.max(within.start());
        let end = match self.valid_to {
            Some(valid_to) if valid_to < within.end() => valid_to.next_day()?,
            _ => within.end(),
        };
        AiUsageDateRange::new(start, end)
    }

    /// The day after the last covered day, or `None` while ongoing (or at the
    /// end of the calendar).
    fn end(&self) -> Option<Date> {
        self.valid_to.and_then(Date::next_day)
    }
}

/// What an admin decides about an account: everything but its identity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiAccountDispositionTerms {
    pub target: AiAccountTarget,
    pub period: AiAccountPeriod,
}

/// A stored disposition of one account.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiAccountDisposition {
    pub id: AiAccountDispositionId,
    pub payer: AiProvider,
    pub account_id: AiProviderAccountId,
    /// The account's login when the disposition was last written.
    pub login: AiProviderLogin,
    pub terms: AiAccountDispositionTerms,
    pub updated_at: OffsetDateTime,
    pub updated_by: Option<UserId>,
}

/// An account's public profile as last read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiStoredProfile {
    pub name: Option<String>,
    pub email: Option<String>,
    pub fetched_at: OffsetDateTime,
}

/// A stored account with its seat history and dispositions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiStoredProviderAccount {
    pub account_id: AiProviderAccountId,
    pub login: AiProviderLogin,
    pub profile: Option<AiStoredProfile>,
    /// Oldest first.
    pub seats: Vec<AiStoredSeat>,
    /// By `valid_from`.
    pub dispositions: Vec<AiAccountDisposition>,
}

impl AiStoredProviderAccount {
    /// The UTC days the account held a seat, through `today` for a seat still
    /// listed, as sorted, disjoint half-open ranges.
    pub fn seated_days(&self, today: Date) -> Vec<AiUsageDateRange> {
        let ranges = self
            .seats
            .iter()
            .filter_map(|seat| {
                let first = seat.first_day();
                let last = seat.last_day().unwrap_or(today);
                (first <= last).then_some((first, last.next_day()?))
            })
            .collect();
        to_ranges(merge(ranges))
    }

    /// The seated days of `within` that none of the account's dispositions
    /// covers.
    pub fn uncovered_seat_days(
        &self,
        today: Date,
        within: AiUsageDateRange,
    ) -> Vec<AiUsageDateRange> {
        let seated = self
            .seated_days(today)
            .into_iter()
            .filter_map(|range| {
                let start = range.start().max(within.start());
                let end = range.end().min(within.end());
                (start < end).then_some((start, end))
            })
            .collect();
        to_ranges(subtract(seated, self.periods()))
    }

    /// The disposition whose period holds `day`.
    pub fn disposition_on(&self, day: Date) -> Option<&AiAccountDisposition> {
        self.dispositions
            .iter()
            .find(|disposition| disposition.terms.period.contains(day))
    }

    /// The period a new disposition of the account defaults to: the days that
    /// need one, overlapping none of its dispositions. An account needs a
    /// disposition on every day it held a seat, and on every day after today
    /// while a seat is listed.
    ///
    /// - It starts the first uncovered stretch of such days that reaches into
    ///   `within` (the days the overview flags), else the first one, extended
    ///   back over earlier stretches that no disposition separates from it.
    ///   Without dispositions that is the first seat's first day.
    /// - It ends the day before the next disposition starts; when none
    ///   follows, it is ongoing while a seat is listed and otherwise ends on
    ///   the last seated day.
    /// - `None` when every such day is covered: an ongoing disposition covers
    ///   a listed seat, or every seat has ended and dispositions cover it.
    ///
    /// An account without seats defaults to today, ongoing.
    pub fn default_period(&self, today: Date, within: AiUsageDateRange) -> Option<AiAccountPeriod> {
        if self.seats.is_empty() {
            return AiAccountPeriod::new(today, None).ok();
        }

        let uncovered = subtract(self.needed_days(), self.periods());
        let &(target, _) = uncovered
            .iter()
            .find(|(start, end)| *start < within.end() && within.start() < *end)
            .or_else(|| uncovered.first())?;
        // Every disposition lies wholly before or after an uncovered day.
        let after_previous = self
            .periods()
            .filter_map(|period| period.valid_to().filter(|valid_to| *valid_to < target))
            .max()
            .and_then(Date::next_day);
        let valid_from = uncovered
            .iter()
            .map(|&(start, _)| start)
            .find(|start| after_previous.is_none_or(|after| *start >= after))?;

        let next_start = self
            .periods()
            .map(|period| period.valid_from())
            .filter(|start| *start > valid_from)
            .min();
        let valid_to = match next_start {
            Some(start) => Some(start.previous_day()?),
            None if self.seats.iter().any(AiStoredSeat::is_listed) => None,
            None => self.seats.iter().filter_map(AiStoredSeat::last_day).max(),
        };
        AiAccountPeriod::new(valid_from, valid_to).ok()
    }

    /// The days the account needs a disposition on, as sorted, disjoint
    /// half-open ranges: the days it held a seat, and every day after today
    /// while a seat is listed (such a range ends at `Date::MAX`).
    fn needed_days(&self) -> Vec<(Date, Date)> {
        let ranges = self
            .seats
            .iter()
            .filter_map(|seat| {
                let end = match seat.last_day() {
                    Some(last_day) => last_day.next_day()?,
                    None => Date::MAX,
                };
                Some((seat.first_day(), end))
            })
            .collect();
        merge(ranges)
    }

    fn periods(&self) -> impl Iterator<Item = AiAccountPeriod> + '_ {
        self.dispositions
            .iter()
            .map(|disposition| disposition.terms.period)
    }
}

/// Sorts half-open ranges and merges those that overlap or touch.
fn merge(mut ranges: Vec<(Date, Date)>) -> Vec<(Date, Date)> {
    ranges.sort();
    let mut merged: Vec<(Date, Date)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// The days of half-open `ranges` that none of `periods` covers.
fn subtract(
    ranges: Vec<(Date, Date)>,
    periods: impl Iterator<Item = AiAccountPeriod>,
) -> Vec<(Date, Date)> {
    let mut pieces = ranges;
    for period in periods {
        let cut_start = period.valid_from();
        let cut_end = period.end();
        pieces = pieces
            .into_iter()
            .flat_map(|(start, end)| {
                let before = (start, end.min(cut_start));
                let after = cut_end.map(|cut_end| (start.max(cut_end), end));
                [Some(before), after]
                    .into_iter()
                    .flatten()
                    .filter(|(start, end)| start < end)
            })
            .collect();
    }
    merge(pieces)
}

fn to_ranges(ranges: Vec<(Date, Date)>) -> Vec<AiUsageDateRange> {
    ranges
        .into_iter()
        .filter_map(|(start, end)| AiUsageDateRange::new(start, end))
        .collect()
}

/// Usage a metered payer paid for, per user and UTC day, personal keys
/// included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AiMeteredUserDay {
    pub user_id: UserId,
    pub day: Date,
    pub usage: AiBillingUsage,
}

/// A user's usage a metered payer paid for over some days.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AiUserActivity {
    /// Days with usage.
    pub days: u32,
    pub last_day: Option<Date>,
    pub usage: AiBillingUsage,
}

impl AiUserActivity {
    /// Adds one day's usage.
    pub fn add(&mut self, day: &AiMeteredUserDay) -> Result<(), AiProviderAccountError> {
        self.usage
            .add(&day.usage)
            .map_err(|_| storage("metered usage totals exceed the supported range"))?;
        self.days += 1;
        self.last_day = self.last_day.max(Some(day.day));
        Ok(())
    }

    /// Larger is more active: priced cost, then records, then days.
    pub fn rank(&self) -> (i64, i64, u32) {
        (
            self.usage.priced_cost.nanos(),
            self.usage.records,
            self.days,
        )
    }
}

/// A payer directory's last refresh attempt and last success.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiDirectoryStatus {
    pub last_attempt_at: OffsetDateTime,
    pub last_success_at: Option<OffsetDateTime>,
    /// Why the last attempt failed; `None` when it succeeded.
    pub last_error: Option<String>,
    /// What the last successful listing held.
    pub seats_listed: Option<u32>,
    /// What the payer reported at the last success.
    pub seats_reported: Option<u32>,
}

/// What one refresh of a directory did. Returned and logged, not stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiDirectoryRefresh {
    pub started_at: OffsetDateTime,
    pub finished_at: OffsetDateTime,
    pub seats_listed: u32,
    pub seats_reported: Option<u32>,
    /// Whether the listing was complete, so seats it omitted were ended.
    pub complete: bool,
    pub seats_started: u32,
    pub seats_ended: u32,
    pub profiles_read: u32,
    pub profiles_failed: u32,
}

/// Whether a scheduled refresh is due at `now`: no refresh succeeded on now's
/// UTC day, and the last attempt, if any, is at least `DIRECTORY_RETRY_AFTER`
/// old. Hourly checks therefore refresh once a day, retry failures hourly, and
/// add no calls when the server restarts.
pub fn directory_refresh_due(status: Option<&AiDirectoryStatus>, now: OffsetDateTime) -> bool {
    let Some(status) = status else {
        return true;
    };
    let today = utc_day(now);
    let succeeded_today = status
        .last_success_at
        .is_some_and(|success| utc_day(success) == today);
    !succeeded_today && now - status.last_attempt_at >= DIRECTORY_RETRY_AFTER
}

fn invalid(message: impl Into<String>) -> AiProviderAccountError {
    AiProviderAccountError::Invalid(message.into())
}

fn storage(message: impl Into<String>) -> AiProviderAccountError {
    AiProviderAccountError::Storage(message.into())
}

#[cfg(test)]
mod tests;

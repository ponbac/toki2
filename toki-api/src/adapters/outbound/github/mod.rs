//! GitHub, through a GitHub App installed on the organization.
//!
//! - The app ("Spinit Toki Billing") has the organization permissions GitHub
//!   Copilot Business: read, for seats, and Administration: read, for billing
//!   usage. It authenticates with a JWT signed by its private key and works
//!   with a one-hour installation token (`app_auth`).
//! - Every GitHub adapter shares one `GithubApp`: one HTTP client, one token
//!   cache and one pacer, which starts at most 10 requests a second, under
//!   GitHub's secondary limit of 900 points a minute for the whole REST API.
//! - `GithubAccountDirectory` is the `ProviderAccountDirectory` of Copilot:
//!   the organization's seats (`GET /orgs/{org}/copilot/billing/seats`, read
//!   page by page by number, never by following `Link`) and public profiles by
//!   numeric id (`GET /user/{id}`), which follows renames.
//! - `GithubMeteredUsageSource` is the `MeteredUsageSource` of Copilot: the
//!   organization's billing usage (`usage`), with GitHub's labels mapped to
//!   Toki's model names in `labels`.
//! - A `401` drops the token and retries once. A `403` or `429` with
//!   `Retry-After` or no remaining requests is a rate limit with its wait.
//! - A seat refresh costs about one seat page and one profile call per
//!   account, plus at most one token an hour: some 40 calls a day against the
//!   installation's 5,000 or more an hour.

mod app_auth;
mod client;
mod labels;
#[cfg(test)]
mod test_support;
mod usage;

pub use usage::GithubMeteredUsageSource;

use std::{collections::HashMap, fmt, sync::Arc};

use async_trait::async_trait;
use serde::Deserialize;
use time::{
    format_description::well_known::Rfc3339, macros::format_description, Date, OffsetDateTime,
};

use crate::domain::{
    models::{
        clip_text, AiListedSeat, AiProviderAccountId, AiProviderAccountProfile, AiProviderLogin,
        AiSeatListing, MAX_ACTIVITY_CLIENT_CHARS, MAX_PROFILE_CHARS,
    },
    ports::outbound::ProviderAccountDirectory,
    ProviderAccountDirectoryError,
};
use client::{GithubAppClient, GithubError};

pub const DEFAULT_API_BASE: &str = "https://api.github.com";
/// Seats per page, GitHub's maximum.
const PAGE_SIZE: u32 = 100;
/// The most pages a listing reads before it gives up.
const MAX_PAGES: u32 = 50;

/// A GitHub App installation, as configuration provides it.
#[derive(Clone, PartialEq, Eq)]
pub struct GithubAppCredentials {
    /// The organization the app is installed on, such as `Spinit-AB`.
    pub organization: String,
    /// The REST API root, such as `DEFAULT_API_BASE`.
    pub api_base: String,
    /// The app's client ID, which issues its JWTs.
    pub client_id: String,
    pub installation_id: u64,
    /// The app's private key in PEM.
    pub private_key_pem: String,
}

impl fmt::Debug for GithubAppCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GithubAppCredentials")
            .field("organization", &self.organization)
            .field("api_base", &self.api_base)
            .field("client_id", &self.client_id)
            .field("installation_id", &self.installation_id)
            .field("private_key_pem", &"[redacted]")
            .finish()
    }
}

/// A GitHub App installation on an organization: the one client, token
/// cache and pacer every GitHub adapter shares.
#[derive(Clone)]
pub struct GithubApp {
    client: Arc<GithubAppClient>,
    organization: String,
}

impl GithubApp {
    /// Fails, with a message naming the setting, when the credentials cannot
    /// be used. Nothing is sent to GitHub yet.
    pub fn new(credentials: GithubAppCredentials) -> Result<Self, String> {
        let organization = credentials.organization.trim().to_string();
        if organization.is_empty()
            || !organization
                .chars()
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '-')
        {
            return Err(format!(
                "github.organization {organization:?} is not a GitHub organization name"
            ));
        }
        Ok(Self {
            client: Arc::new(GithubAppClient::new(&credentials)?),
            organization,
        })
    }
}

/// The Copilot seats of a GitHub organization, as a `ProviderAccountDirectory`.
pub struct GithubAccountDirectory {
    client: Arc<GithubAppClient>,
    organization: String,
    page_size: u32,
}

impl GithubAccountDirectory {
    pub fn new(app: GithubApp) -> Self {
        Self {
            client: app.client,
            organization: app.organization,
            page_size: PAGE_SIZE,
        }
    }

    #[cfg(test)]
    fn with_page_size(self, page_size: u32) -> Self {
        Self { page_size, ..self }
    }

    /// GitHub's answers to the seat listing, in the organization's terms.
    fn seats_error(&self, error: GithubError) -> ProviderAccountDirectoryError {
        let organization = &self.organization;
        match error {
            GithubError::NotFound(_) => ProviderAccountDirectoryError::NotFound(format!(
                "{organization} not found, or the app is not installed on it"
            )),
            GithubError::Unprocessable(_) => ProviderAccountDirectoryError::Forbidden(format!(
                "Copilot Business is not enabled or not set up for {organization}"
            )),
            GithubError::Forbidden(message) => ProviderAccountDirectoryError::Forbidden(format!(
                "GitHub refused to list {organization}'s Copilot seats ({message}); the app \
                 needs the organization permission GitHub Copilot Business: read"
            )),
            other => other.into(),
        }
    }
}

/// One page of `GET /orgs/{org}/copilot/billing/seats`.
#[derive(Deserialize)]
struct SeatsPage {
    total_seats: Option<u64>,
    #[serde(default)]
    seats: Vec<Seat>,
}

#[derive(Deserialize)]
struct Seat {
    created_at: String,
    assignee: Option<Assignee>,
    pending_cancellation_date: Option<String>,
    last_activity_at: Option<String>,
    last_activity_editor: Option<String>,
}

/// A seat's assignee: a user, or for other assignment kinds something else.
#[derive(Deserialize)]
struct Assignee {
    login: Option<String>,
    id: Option<i64>,
    #[serde(rename = "type")]
    kind: Option<String>,
}

/// `GET /user/{id}`.
#[derive(Deserialize)]
struct User {
    login: String,
    id: i64,
    name: Option<String>,
    email: Option<String>,
}

fn instant(raw: &str) -> Result<OffsetDateTime, GithubError> {
    OffsetDateTime::parse(raw, &Rfc3339)
        .map_err(|_| GithubError::Response(format!("invalid timestamp {raw:?}")))
}

fn day(raw: &str) -> Result<Date, GithubError> {
    Date::parse(raw, format_description!("[year]-[month]-[day]"))
        .map_err(|_| GithubError::Response(format!("invalid date {raw:?}")))
}

/// A user's seat, or `None` for an assignee that is not a user with a numeric
/// id and a valid login.
fn listed_seat(seat: Seat) -> Result<Option<AiListedSeat>, GithubError> {
    let Some(Assignee {
        login: Some(login),
        id: Some(id),
        kind: Some(kind),
    }) = seat.assignee
    else {
        return Ok(None);
    };
    let (Some(account_id), Ok(login), "User") = (
        AiProviderAccountId::new(id),
        AiProviderLogin::parse(&login),
        kind.as_str(),
    ) else {
        return Ok(None);
    };

    Ok(Some(AiListedSeat {
        account_id,
        login,
        created_at: instant(&seat.created_at)?,
        pending_cancellation_on: seat
            .pending_cancellation_date
            .as_deref()
            .map(day)
            .transpose()?,
        last_activity_at: seat.last_activity_at.as_deref().map(instant).transpose()?,
        last_activity_client: clip_text(
            seat.last_activity_editor.as_deref(),
            MAX_ACTIVITY_CLIENT_CHARS,
        ),
    }))
}

#[async_trait]
impl ProviderAccountDirectory for GithubAccountDirectory {
    async fn seats(&self) -> Result<AiSeatListing, ProviderAccountDirectoryError> {
        let mut seats: Vec<AiListedSeat> = Vec::new();
        let mut positions: HashMap<AiProviderAccountId, usize> = HashMap::new();
        let mut reported_total = None;
        let mut skipped = 0_usize;

        // A listing ends on a short page, an empty one included, so an
        // incomplete read is never taken for a complete one.
        for page in 1..=MAX_PAGES {
            let path = format!(
                "/orgs/{}/copilot/billing/seats?per_page={}&page={page}",
                self.organization, self.page_size
            );
            let response = self
                .client
                .get(&path)
                .await
                .map_err(|error| self.seats_error(error))?;
            let parsed: SeatsPage = response.json()?;
            if page == 1 {
                reported_total = parsed
                    .total_seats
                    .map(|total| u32::try_from(total).unwrap_or(u32::MAX));
            }
            let on_page = parsed.seats.len();
            for seat in parsed.seats {
                match listed_seat(seat)? {
                    Some(seat) => match positions.get(&seat.account_id) {
                        // An account listed twice: the later entry wins.
                        Some(&position) => seats[position] = seat,
                        None => {
                            positions.insert(seat.account_id, seats.len());
                            seats.push(seat);
                        }
                    },
                    None => skipped += 1,
                }
            }

            if on_page < self.page_size as usize {
                if skipped > 0 {
                    tracing::info!(
                        skipped,
                        organization = %self.organization,
                        "skipped Copilot seats not assigned to a user"
                    );
                }
                return Ok(AiSeatListing {
                    seats,
                    reported_total,
                    ended_on_short_page: true,
                });
            }
        }

        Err(ProviderAccountDirectoryError::Response(format!(
            "{}'s Copilot seats did not end within {MAX_PAGES} pages",
            self.organization
        )))
    }

    async fn profile(
        &self,
        account: AiProviderAccountId,
    ) -> Result<Option<AiProviderAccountProfile>, ProviderAccountDirectoryError> {
        let response = match self.client.get(&format!("/user/{account}")).await {
            Ok(response) => response,
            Err(GithubError::NotFound(_)) => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let user: User = response.json()?;
        if user.id != account.as_i64() {
            return Err(ProviderAccountDirectoryError::Response(format!(
                "GitHub answered user {account} with user {}",
                user.id
            )));
        }
        let login = AiProviderLogin::parse(&user.login).map_err(|_| {
            ProviderAccountDirectoryError::Response(format!("user {account} has an invalid login"))
        })?;

        Ok(Some(AiProviderAccountProfile {
            account_id: account,
            login,
            name: clip_text(user.name.as_deref(), MAX_PROFILE_CHARS),
            email: clip_text(user.email.as_deref(), MAX_PROFILE_CHARS),
        }))
    }
}

#[cfg(test)]
mod tests;

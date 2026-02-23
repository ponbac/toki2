use anyhow::{Context, Result};
use reqwest::{Client, StatusCode};
use serde::Serialize;
use std::sync::{Arc, Mutex};
use time::OffsetDateTime;

use crate::types::{Me, TimerHistoryEntry};

/// Cookie name used by tower-sessions (default).
const SESSION_COOKIE: &str = "id";

#[derive(Debug, Clone)]
pub struct ApiClient {
    client: Client,
    base_url: String,
    session_id: String,
    /// When true, all API calls mutate/read this in-memory store instead of hitting the server.
    dev_history: Option<Arc<Mutex<Vec<TimerHistoryEntry>>>>,
}

impl ApiClient {
    /// Create a new client with the given session cookie.
    pub fn new(base_url: &str, session_id: &str) -> Result<Self> {
        let client = Client::builder()
            .cookie_store(true)
            .build()
            .context("Failed to build HTTP client")?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            session_id: session_id.to_string(),
            dev_history: None,
        })
    }

    /// Create a dev-mode client that returns fake data without hitting any server.
    pub fn dev() -> Result<Self> {
        let client = Client::builder().build().context("Failed to build HTTP client")?;
        Ok(Self {
            client,
            base_url: String::new(),
            session_id: String::new(),
            dev_history: Some(Arc::new(Mutex::new(dev_history()))),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Add the session cookie to a request.
    fn with_session(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        req.header(
            "Cookie",
            format!("{}={}", SESSION_COOKIE, self.session_id),
        )
    }

    /// GET /me — verify session is valid, return current user info.
    pub async fn me(&self) -> Result<Me> {
        if self.dev_history.is_some() {
            return Ok(Me {
                id: 1,
                email: "dev@localhost".to_string(),
                full_name: "Dev User".to_string(),
            });
        }

        let resp = self
            .with_session(self.client.get(self.url("/me")))
            .send()
            .await
            .context("Failed to call /me")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            anyhow::bail!("Session expired or invalid. Run `toki-tui --login` to authenticate.");
        }
        resp.error_for_status_ref()
            .context("GET /me returned error")?;
        resp.json::<Me>().await.context("Failed to parse /me response")
    }

    /// GET /time-tracking/timer-history — returns all history for the authenticated user.
    pub async fn get_timer_history(&self) -> Result<Vec<TimerHistoryEntry>> {
        if let Some(store) = &self.dev_history {
            let mut list = store.lock().unwrap().clone();
            list.sort_by(|a, b| b.start_time.cmp(&a.start_time));
            return Ok(list);
        }

        let resp = self
            .with_session(self.client.get(self.url("/time-tracking/timer-history")))
            .send()
            .await
            .context("Failed to call /time-tracking/timer-history")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            anyhow::bail!("Session expired. Run `toki-tui --login` to re-authenticate.");
        }
        resp.error_for_status_ref()
            .context("GET /time-tracking/timer-history returned error")?;
        resp.json::<Vec<TimerHistoryEntry>>()
            .await
            .context("Failed to parse timer history response")
    }

    /// POST /time-tracking/timer — save a completed timer entry.
    pub async fn save_timer_entry(
        &self,
        start_time: OffsetDateTime,
        end_time: OffsetDateTime,
        project_id: Option<String>,
        project_name: Option<String>,
        activity_id: Option<String>,
        activity_name: Option<String>,
        note: Option<String>,
    ) -> Result<TimerHistoryEntry> {
        if let Some(store) = &self.dev_history {
            let entry = TimerHistoryEntry {
                id: store.lock().unwrap().iter().map(|e| e.id).max().unwrap_or(0) + 1,
                user_id: 1,
                start_time,
                end_time: Some(end_time),
                project_id,
                project_name,
                activity_id,
                activity_name,
                note,
            };
            store.lock().unwrap().push(entry.clone());
            return Ok(entry);
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body {
            #[serde(with = "time::serde::rfc3339")]
            start_time: OffsetDateTime,
            #[serde(with = "time::serde::rfc3339")]
            end_time: OffsetDateTime,
            project_id: Option<String>,
            project_name: Option<String>,
            activity_id: Option<String>,
            activity_name: Option<String>,
            note: Option<String>,
        }
        let body = Body {
            start_time,
            end_time,
            project_id,
            project_name,
            activity_id,
            activity_name,
            note,
        };
        let resp = self
            .with_session(self.client.post(self.url("/time-tracking/timer")))
            .json(&body)
            .send()
            .await
            .context("Failed to call POST /time-tracking/timer")?;
        resp.error_for_status_ref()
            .context("POST /time-tracking/timer returned error")?;
        resp.json::<TimerHistoryEntry>()
            .await
            .context("Failed to parse save response")
    }

    /// PUT /time-tracking/timer/:id — update an existing timer entry.
    pub async fn update_timer_entry(
        &self,
        entry_id: i32,
        start_time: OffsetDateTime,
        end_time: OffsetDateTime,
        project_id: Option<String>,
        project_name: Option<String>,
        activity_id: Option<String>,
        activity_name: Option<String>,
        note: Option<String>,
    ) -> Result<TimerHistoryEntry> {
        if let Some(store) = &self.dev_history {
            let updated = TimerHistoryEntry {
                id: entry_id,
                user_id: 1,
                start_time,
                end_time: Some(end_time),
                project_id,
                project_name,
                activity_id,
                activity_name,
                note,
            };
            let mut list = store.lock().unwrap();
            if let Some(e) = list.iter_mut().find(|e| e.id == entry_id) {
                *e = updated.clone();
            }
            return Ok(updated);
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body {
            #[serde(with = "time::serde::rfc3339")]
            start_time: OffsetDateTime,
            #[serde(with = "time::serde::rfc3339")]
            end_time: OffsetDateTime,
            project_id: Option<String>,
            project_name: Option<String>,
            activity_id: Option<String>,
            activity_name: Option<String>,
            note: Option<String>,
        }
        let body = Body {
            start_time,
            end_time,
            project_id,
            project_name,
            activity_id,
            activity_name,
            note,
        };
        let resp = self
            .with_session(
                self.client
                    .put(self.url(&format!("/time-tracking/timer/{}", entry_id))),
            )
            .json(&body)
            .send()
            .await
            .context("Failed to call PUT /time-tracking/timer")?;
        resp.error_for_status_ref()
            .context("PUT /time-tracking/timer returned error")?;
        resp.json::<TimerHistoryEntry>()
            .await
            .context("Failed to parse update response")
    }
}

/// Generate a handful of fake timer history entries for dev mode.
fn dev_history() -> Vec<TimerHistoryEntry> {
    use time::macros::offset;
    let now = OffsetDateTime::now_utc().to_offset(offset!(+1));
    let today = now.date();

    let entry = |id: i32, h_start: u8, h_end: u8, pid: &str, pname: &str, aid: &str, aname: &str, note: &str| {
        let start = OffsetDateTime::new_in_offset(
            today,
            time::Time::from_hms(h_start, 0, 0).unwrap(),
            offset!(+1),
        );
        let end = OffsetDateTime::new_in_offset(
            today,
            time::Time::from_hms(h_end, 0, 0).unwrap(),
            offset!(+1),
        );
        TimerHistoryEntry {
            id,
            user_id: 1,
            start_time: start,
            end_time: Some(end),
            project_id: Some(pid.to_string()),
            project_name: Some(pname.to_string()),
            activity_id: Some(aid.to_string()),
            activity_name: Some(aname.to_string()),
            note: Some(note.to_string()),
        }
    };

    vec![
        entry(1, 8, 10, "proj_1", "Nordic Crisis Manager", "act_1_1", "Backend Development", "API refactor"),
        entry(2, 10, 12, "proj_1", "Nordic Crisis Manager", "act_1_4", "Code Review", "PR review"),
        entry(3, 13, 15, "proj_2", "Azure DevOps Integration", "act_2_1", "API Integration", "Webhook setup"),
        entry(4, 15, 17, "proj_3", "TUI Development", "act_3_2", "Feature Implementation", "Scrollable lists"),
    ]
}

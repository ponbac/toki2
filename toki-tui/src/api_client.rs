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
    mt_cookies: Vec<(String, String)>,
    /// When true, all API calls mutate/read this in-memory store instead of hitting the server.
    dev_history: Option<Arc<Mutex<Vec<TimerHistoryEntry>>>>,
}

impl ApiClient {
    /// Create a new client with the given session cookie.
    pub fn new(base_url: &str, session_id: &str, mt_cookies: Vec<(String, String)>) -> Result<Self> {
        let client = Client::builder()
            .cookie_store(true)
            .build()
            .context("Failed to build HTTP client")?;
        Ok(Self {
            client,
            base_url: base_url.trim_end_matches('/').to_string(),
            session_id: session_id.to_string(),
            mt_cookies,
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
            mt_cookies: vec![],
            dev_history: Some(Arc::new(Mutex::new(dev_history()))),
        })
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    /// Add the session cookie to a request.
    fn with_session(&self, req: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        let mut cookie_header = format!("{}={}", SESSION_COOKIE, self.session_id);
        for (name, value) in &self.mt_cookies {
            cookie_header.push_str(&format!("; {}={}", name, value));
        }
        req.header("Cookie", cookie_header)
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

    /// GET /time-tracking/timer — fetch the currently running timer (if any).
    pub async fn get_active_timer(&mut self) -> Result<Option<crate::types::ActiveTimerState>> {
        if self.dev_history.is_some() {
            return Ok(None); // dev mode: no active timer on startup
        }

        let resp = self
            .with_session(self.client.get(self.url("/time-tracking/timer")))
            .send()
            .await
            .context("Failed to call GET /time-tracking/timer")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            anyhow::bail!("Session expired. Run `toki-tui --login` to re-authenticate.");
        }
        resp.error_for_status_ref()
            .context("GET /time-tracking/timer returned error")?;

        let new_cookies = extract_set_cookies(&resp);
        let result = resp
            .json::<crate::types::GetTimerResponse>()
            .await
            .context("Failed to parse GET /time-tracking/timer response")?;

        if self.update_mt_cookies(new_cookies) {
            crate::config::TokiConfig::save_mt_cookies(&self.mt_cookies)?;
        }

        Ok(result.timer)
    }

    /// GET /time-tracking/time-info — fetch scheduled/worked/flex hours for a date range.
    pub async fn get_time_info(&mut self, from: time::Date, to: time::Date) -> Result<crate::types::TimeInfo> {
        if self.dev_history.is_some() {
            // Dev mode: return a sensible default matching 32h/week
            return Ok(crate::types::TimeInfo {
                period_time_left: 6.0,
                worked_period_time: 26.0,
                scheduled_period_time: 32.0,
                worked_period_with_absence_time: 26.0,
                flex_time_current: 1.5,
            });
        }

        let format = time::format_description::parse("[year]-[month]-[day]").unwrap();
        let from_str = from.format(&format).context("Failed to format from date")?;
        let to_str = to.format(&format).context("Failed to format to date")?;

        let resp = self
            .with_session(self.client.get(self.url("/time-tracking/time-info")))
            .query(&[("from", &from_str), ("to", &to_str)])
            .send()
            .await
            .context("Failed to call GET /time-tracking/time-info")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            anyhow::bail!("Session expired. Run `toki-tui --login` to re-authenticate.");
        }
        resp.error_for_status_ref()
            .context("GET /time-tracking/time-info returned error")?;

        let new_cookies = extract_set_cookies(&resp);
        let result = resp
            .json::<crate::types::TimeInfo>()
            .await
            .context("Failed to parse time-info response")?;

        if self.update_mt_cookies(new_cookies) {
            crate::config::TokiConfig::save_mt_cookies(&self.mt_cookies)?;
        }

        Ok(result)
    }

    /// GET /time-tracking/time-entries — fetch Milltime-authoritative entries for a date range.
    pub async fn get_time_entries(
        &mut self,
        from: time::Date,
        to: time::Date,
    ) -> Result<Vec<crate::types::TimeEntry>> {
        if let Some(dev) = &self.dev_history {
            // In dev mode, synthesize TimeEntry records from the local stub data
            let store = dev.lock().unwrap().clone();
            return Ok(store
                .iter()
                .filter_map(|e| {
                    let reg_id = e.registration_id.clone()?;
                    Some(crate::types::TimeEntry {
                        registration_id: reg_id,
                        project_id: e.project_id.clone().unwrap_or_default(),
                        project_name: e.project_name.clone().unwrap_or_default(),
                        activity_id: e.activity_id.clone().unwrap_or_default(),
                        activity_name: e.activity_name.clone().unwrap_or_default(),
                        date: {
                            let d = e.start_time.date();
                            format!("{:04}-{:02}-{:02}", d.year(), d.month() as u8, d.day())
                        },
                        hours: e.end_time.map(|end| {
                            (end - e.start_time).whole_seconds() as f64 / 3600.0
                        }).unwrap_or(0.0),
                        note: e.note.clone(),
                        start_time: Some(e.start_time),
                        end_time: e.end_time,
                        week_number: e.start_time.iso_week(),
                    })
                })
                .collect());
        }

        let fmt = time::format_description::parse("[year]-[month]-[day]")
            .context("Failed to build date format")?;
        let from_str = from.format(&fmt).context("Failed to format from date")?;
        let to_str = to.format(&fmt).context("Failed to format to date")?;

        let resp = self
            .with_session(
                self.client
                    .get(self.url("/time-tracking/time-entries"))
                    .query(&[("from", &from_str), ("to", &to_str)]),
            )
            .send()
            .await
            .context("Failed to call GET /time-tracking/time-entries")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            anyhow::bail!("Session expired. Run `toki-tui --login` to re-authenticate.");
        }
        resp.error_for_status_ref()
            .context("GET /time-tracking/time-entries returned error")?;

        let new_cookies = extract_set_cookies(&resp);
        let entries: Vec<crate::types::TimeEntry> = resp
            .json()
            .await
            .context("Failed to parse GET /time-tracking/time-entries response")?;
        if self.update_mt_cookies(new_cookies) {
            crate::config::TokiConfig::save_mt_cookies(&self.mt_cookies)?;
        }
        Ok(entries)
    }


    pub async fn start_timer(
        &mut self,
        project_id: Option<String>,
        project_name: Option<String>,
        activity_id: Option<String>,
        activity_name: Option<String>,
        note: Option<String>,
    ) -> Result<()> {
        if self.dev_history.is_some() {
            return Ok(()); // dev mode: timer is local-only
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body {
            project_id: Option<String>,
            project_name: Option<String>,
            activity_id: Option<String>,
            activity_name: Option<String>,
            user_note: Option<String>,
        }
        let resp = self
            .with_session(self.client.post(self.url("/time-tracking/timer")))
            .json(&Body { project_id, project_name, activity_id, activity_name, user_note: note })
            .send()
            .await
            .context("Failed to call POST /time-tracking/timer")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            anyhow::bail!("Session expired. Run `toki-tui --login` to re-authenticate.");
        }
        resp.error_for_status_ref()
            .context("POST /time-tracking/timer returned error")?;

        let new_cookies = extract_set_cookies(&resp);
        let _ = resp.bytes().await;
        if self.update_mt_cookies(new_cookies) {
            crate::config::TokiConfig::save_mt_cookies(&self.mt_cookies)?;
        }
        Ok(())
    }

    /// PUT /time-tracking/timer — save the active timer to Milltime and stop it.
    /// The server uses the project/activity already stored on the active timer.
    pub async fn save_timer(&mut self, note: Option<String>) -> Result<()> {
        if self.dev_history.is_some() {
            return Ok(()); // dev mode: no-op
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body { user_note: Option<String> }
        let resp = self
            .with_session(self.client.put(self.url("/time-tracking/timer")))
            .json(&Body { user_note: note })
            .send()
            .await
            .context("Failed to call PUT /time-tracking/timer")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            anyhow::bail!("Session expired. Run `toki-tui --login` to re-authenticate.");
        }
        resp.error_for_status_ref()
            .context("PUT /time-tracking/timer returned error")?;

        let new_cookies = extract_set_cookies(&resp);
        let _ = resp.bytes().await;
        if self.update_mt_cookies(new_cookies) {
            crate::config::TokiConfig::save_mt_cookies(&self.mt_cookies)?;
        }
        Ok(())
    }

    /// DELETE /time-tracking/timer — discard the active timer without registering to Milltime.
    pub async fn stop_timer(&mut self) -> Result<()> {
        if self.dev_history.is_some() {
            return Ok(());
        }

        let resp = self
            .with_session(self.client.delete(self.url("/time-tracking/timer")))
            .send()
            .await
            .context("Failed to call DELETE /time-tracking/timer")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            anyhow::bail!("Session expired. Run `toki-tui --login` to re-authenticate.");
        }
        resp.error_for_status_ref()
            .context("DELETE /time-tracking/timer returned error")?;

        let new_cookies = extract_set_cookies(&resp);
        let _ = resp.bytes().await;
        if self.update_mt_cookies(new_cookies) {
            crate::config::TokiConfig::save_mt_cookies(&self.mt_cookies)?;
        }
        Ok(())
    }

    /// PUT /time-tracking/update-timer — update fields on the currently running timer.
    /// Any None field is left unchanged on the server (server merges with current state).
    pub async fn update_active_timer(
        &mut self,
        project_id: Option<String>,
        project_name: Option<String>,
        activity_id: Option<String>,
        activity_name: Option<String>,
        note: Option<String>,
        start_time: Option<time::OffsetDateTime>,
    ) -> Result<()> {
        if self.dev_history.is_some() {
            return Ok(());
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body {
            project_id: Option<String>,
            project_name: Option<String>,
            activity_id: Option<String>,
            activity_name: Option<String>,
            user_note: Option<String>,
            #[serde(
                skip_serializing_if = "Option::is_none",
                with = "time::serde::rfc3339::option"
            )]
            start_time: Option<time::OffsetDateTime>,
        }
        let resp = self
            .with_session(self.client.put(self.url("/time-tracking/update-timer")))
            .json(&Body { project_id, project_name, activity_id, activity_name, user_note: note, start_time })
            .send()
            .await
            .context("Failed to call PUT /time-tracking/update-timer")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            anyhow::bail!("Session expired. Run `toki-tui --login` to re-authenticate.");
        }
        resp.error_for_status_ref()
            .context("PUT /time-tracking/update-timer returned error")?;

        let new_cookies = extract_set_cookies(&resp);
        let _ = resp.bytes().await;
        if self.update_mt_cookies(new_cookies) {
            crate::config::TokiConfig::save_mt_cookies(&self.mt_cookies)?;
        }
        Ok(())
    }

    /// PUT /time-tracking/time-entries — edit an existing saved time entry.
    pub async fn edit_time_entry(
        &mut self,
        project_registration_id: &str,
        project_id: &str,
        project_name: &str,
        activity_id: &str,
        activity_name: &str,
        start_time: time::OffsetDateTime,
        end_time: time::OffsetDateTime,
        reg_day: &str,
        week_number: i32,
        user_note: &str,
        original_project_id: Option<&str>,
        original_activity_id: Option<&str>,
    ) -> Result<()> {
        if self.dev_history.is_some() {
            return Ok(());
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            project_registration_id: &'a str,
            project_id: &'a str,
            project_name: &'a str,
            activity_id: &'a str,
            activity_name: &'a str,
            start_time: String,
            end_time: String,
            reg_day: &'a str,
            week_number: i32,
            user_note: &'a str,
            original_reg_day: Option<String>,
            original_project_id: Option<&'a str>,
            original_activity_id: Option<&'a str>,
        }

        let fmt = time::format_description::well_known::Rfc3339;
        let body = Body {
            project_registration_id,
            project_id,
            project_name,
            activity_id,
            activity_name,
            start_time: start_time.format(&fmt).context("Failed to format start_time")?,
            end_time: end_time.format(&fmt).context("Failed to format end_time")?,
            reg_day,
            week_number,
            user_note,
            original_reg_day: None,
            original_project_id,
            original_activity_id,
        };

        let resp = self
            .with_session(self.client.put(self.url("/time-tracking/time-entries")))
            .json(&body)
            .send()
            .await
            .context("Failed to call PUT /time-tracking/time-entries")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            anyhow::bail!("Session expired. Run `toki-tui --login` to re-authenticate.");
        }
        resp.error_for_status_ref()
            .context("PUT /time-tracking/time-entries returned error")?;

        let new_cookies = extract_set_cookies(&resp);
        let _ = resp.bytes().await;
        if self.update_mt_cookies(new_cookies) {
            crate::config::TokiConfig::save_mt_cookies(&self.mt_cookies)?;
        }
        Ok(())
    }

    /// DELETE /time-tracking/time-entries — permanently delete a saved time entry.
    pub async fn delete_time_entry(&mut self, registration_id: &str) -> Result<()> {
        if self.dev_history.is_some() {
            // Dev mode: remove from in-memory store
            if let Some(dev) = &self.dev_history {
                dev.lock().unwrap().retain(|e| {
                    e.registration_id.as_deref() != Some(registration_id)
                });
            }
            return Ok(());
        }

        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            project_registration_id: &'a str,
        }

        let resp = self
            .with_session(self.client.delete(self.url("/time-tracking/time-entries")))
            .json(&Body { project_registration_id: registration_id })
            .send()
            .await
            .context("Failed to call DELETE /time-tracking/time-entries")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            anyhow::bail!("Session expired. Run `toki-tui --login` to re-authenticate.");
        }
        resp.error_for_status_ref()
            .context("DELETE /time-tracking/time-entries returned error")?;

        let new_cookies = extract_set_cookies(&resp);
        let _ = resp.bytes().await;
        if self.update_mt_cookies(new_cookies) {
            crate::config::TokiConfig::save_mt_cookies(&self.mt_cookies)?;
        }
        Ok(())
    }

    /// POST /time-tracking/authenticate — exchange username/password for Milltime cookies.
    /// Returns the Set-Cookie values as (name, value) pairs.
    pub async fn authenticate(
        &self,
        username: &str,
        password: &str,
    ) -> Result<Vec<(String, String)>> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Body<'a> {
            username: &'a str,
            password: &'a str,
        }
        let resp = self
            .with_session(
                self.client
                    .post(self.url("/time-tracking/authenticate"))
            )
            .json(&Body { username, password })
            .send()
            .await
            .context("Failed to call POST /time-tracking/authenticate")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            anyhow::bail!("Invalid Milltime credentials.");
        }
        resp.error_for_status_ref()
            .context("POST /time-tracking/authenticate returned error")?;

        Ok(extract_set_cookies(&resp))
    }

    /// Merge new cookies into self.mt_cookies (update existing names, add new).
    /// Returns true if any cookie changed (caller should persist).
    pub fn update_mt_cookies(&mut self, new_cookies: Vec<(String, String)>) -> bool {
        if new_cookies.is_empty() {
            return false;
        }
        for (name, value) in new_cookies {
            if let Some(existing) = self.mt_cookies.iter_mut().find(|(n, _)| n == &name) {
                existing.1 = value;
            } else {
                self.mt_cookies.push((name, value));
            }
        }
        true
    }

    /// Return the current Milltime cookies (for persisting).
    pub fn mt_cookies(&self) -> &[(String, String)] {
        &self.mt_cookies
    }

    /// GET /time-tracking/projects — returns all projects for the authenticated user.
    pub async fn get_projects(&mut self) -> Result<Vec<crate::types::Project>> {
        if self.dev_history.is_some() {
            return Ok(vec![
                crate::types::Project { id: "proj_1".to_string(), name: "Nordic Crisis Manager".to_string() },
                crate::types::Project { id: "proj_2".to_string(), name: "Azure DevOps Integration".to_string() },
                crate::types::Project { id: "proj_3".to_string(), name: "TUI Development".to_string() },
            ]);
        }

        let resp = self
            .with_session(self.client.get(self.url("/time-tracking/projects")))
            .send()
            .await
            .context("Failed to call GET /time-tracking/projects")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            anyhow::bail!("Session expired. Run `toki-tui --login` to re-authenticate.");
        }
        resp.error_for_status_ref()
            .context("GET /time-tracking/projects returned error")?;

        let new_cookies = extract_set_cookies(&resp);
        let dtos = resp
            .json::<Vec<ProjectDto>>()
            .await
            .context("Failed to parse projects response")?;

        if self.update_mt_cookies(new_cookies) {
            crate::config::TokiConfig::save_mt_cookies(&self.mt_cookies)?;
        }

        let mut projects: Vec<crate::types::Project> = dtos
            .into_iter()
            .map(|d| crate::types::Project { id: d.project_id, name: d.project_name })
            .collect();
        projects.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(projects)
    }

    /// GET /time-tracking/projects/:id/activities — returns activities for a project.
    pub async fn get_activities(&mut self, project_id: &str) -> Result<Vec<crate::types::Activity>> {
        if self.dev_history.is_some() {
            let activities = vec![
                crate::types::Activity { id: "act_1_1".to_string(), name: "Backend Development".to_string(), project_id: project_id.to_string() },
                crate::types::Activity { id: "act_1_4".to_string(), name: "Code Review".to_string(), project_id: project_id.to_string() },
            ];
            return Ok(activities);
        }

        let resp = self
            .with_session(self.client.get(self.url(&format!("/time-tracking/projects/{}/activities", project_id))))
            .send()
            .await
            .context("Failed to call GET /time-tracking/projects/:id/activities")?;

        if resp.status() == StatusCode::UNAUTHORIZED {
            anyhow::bail!("Session expired. Run `toki-tui --login` to re-authenticate.");
        }
        resp.error_for_status_ref()
            .context("GET /time-tracking/projects/:id/activities returned error")?;

        let new_cookies = extract_set_cookies(&resp);
        let dtos = resp
            .json::<Vec<ActivityDto>>()
            .await
            .context("Failed to parse activities response")?;

        if self.update_mt_cookies(new_cookies) {
            crate::config::TokiConfig::save_mt_cookies(&self.mt_cookies)?;
        }

        let mut activities: Vec<crate::types::Activity> = dtos
            .into_iter()
            .map(|d| crate::types::Activity {
                id: d.activity,
                name: d.activity_name,
                project_id: project_id.to_string(),
            })
            .collect();
        activities.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(activities)
    }
}

/// Parse Set-Cookie headers from a response into (name, value) pairs.
/// Strips all cookie attributes (path, domain, expires, httponly, secure, samesite).
fn extract_set_cookies(resp: &reqwest::Response) -> Vec<(String, String)> {
    resp.headers()
        .get_all(reqwest::header::SET_COOKIE)
        .iter()
        .filter_map(|v| {
            let s = v.to_str().ok()?;
            // Take only the first segment (before any ';') which is "name=value"
            let pair = s.split(';').next()?;
            let mut parts = pair.splitn(2, '=');
            let name = parts.next()?.trim().to_string();
            let value = parts.next()?.trim().to_string();
            if name.is_empty() { None } else { Some((name, value)) }
        })
        .collect()
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectDto {
    project_id: String,
    project_name: String,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActivityDto {
    activity: String,
    activity_name: String,
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
            registration_id: None,
        }
    };

    vec![
        entry(1, 8, 10, "proj_1", "Nordic Crisis Manager", "act_1_1", "Backend Development", "API refactor"),
        entry(2, 10, 12, "proj_1", "Nordic Crisis Manager", "act_1_4", "Code Review", "PR review"),
        entry(3, 13, 15, "proj_2", "Azure DevOps Integration", "act_2_1", "API Integration", "Webhook setup"),
        entry(4, 15, 17, "proj_3", "TUI Development", "act_3_2", "Feature Implementation", "Scrollable lists"),
    ]
}

use anyhow::{Context, Result};
use reqwest::{
    cookie::{CookieStore, Jar},
    Client, RequestBuilder, Response, StatusCode, Url,
};
use serde::de::DeserializeOwned;
use std::sync::Arc;

use crate::api::dev_backend::DevBackend;
use crate::api::dto::{
    ActivityDto, AuthenticateRequest, DeleteEntryRequest, EditEntryRequest, ProjectDto,
    SaveTimerRequest, StartTimerRequest, UpdateActiveTimerRequest,
};
use crate::session_store;
use crate::types::{
    ActiveTimerState, Activity, GetTimerResponse, Me, Project, TimeEntry, TimeInfo,
};

const SESSION_COOKIE: &str = "id";
const UNAUTH_INVALID_SESSION: &str =
    "Session expired or invalid. Run `toki-tui login` to authenticate.";
const UNAUTH_RELOGIN: &str = "Session expired. Run `toki-tui login` to re-authenticate.";
const UNAUTH_INVALID_MILLTIME_CREDENTIALS: &str = "Invalid Milltime credentials.";

#[derive(Debug, Clone)]
pub struct ApiClient {
    client: Client,
    base_url: Url,
    jar: Arc<Jar>,
    mt_cookies: Vec<(String, String)>,
    dev_backend: Option<DevBackend>,
}

impl ApiClient {
    pub fn new(
        base_url: &str,
        session_id: &str,
        mt_cookies: Vec<(String, String)>,
    ) -> Result<Self> {
        let base_url = Url::parse(base_url.trim_end_matches('/'))
            .with_context(|| format!("Invalid API URL: {}", base_url))?;
        let jar = Arc::new(Jar::default());

        jar.add_cookie_str(
            &format!("{}={}; Path=/", SESSION_COOKIE, session_id),
            &base_url,
        );
        for (name, value) in &mt_cookies {
            jar.add_cookie_str(&format!("{}={}; Path=/", name, value), &base_url);
        }

        let client = Client::builder()
            .cookie_provider(jar.clone())
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            client,
            base_url,
            jar,
            mt_cookies,
            dev_backend: None,
        })
    }

    pub fn dev() -> Result<Self> {
        let base_url = Url::parse("http://localhost")?;
        let jar = Arc::new(Jar::default());
        let client = Client::builder()
            .cookie_provider(jar.clone())
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            client,
            base_url,
            jar,
            mt_cookies: vec![],
            dev_backend: Some(DevBackend::new()),
        })
    }

    fn endpoint(&self, path: &str) -> Result<Url> {
        self.base_url
            .join(path)
            .with_context(|| format!("Failed to build URL for path {}", path))
    }

    fn sync_mt_cookies_from_jar(&mut self) -> Result<()> {
        let Some(header_value) = self.jar.cookies(&self.base_url) else {
            return Ok(());
        };

        let header = header_value
            .to_str()
            .context("Invalid cookie header from cookie jar")?;

        let mut cookies = header
            .split(';')
            .filter_map(|segment| {
                let pair = segment.trim();
                if pair.is_empty() {
                    return None;
                }

                let mut parts = pair.splitn(2, '=');
                let name = parts.next()?.trim();
                let value = parts.next()?.trim();
                if name.is_empty() || name == SESSION_COOKIE {
                    return None;
                }

                Some((name.to_string(), value.to_string()))
            })
            .collect::<Vec<_>>();

        cookies.sort_by(|a, b| a.0.cmp(&b.0));

        if cookies != self.mt_cookies {
            self.mt_cookies = cookies;
            session_store::save_mt_cookies(&self.mt_cookies)?;
        }

        Ok(())
    }

    async fn send(
        &mut self,
        request: RequestBuilder,
        call_name: &str,
        unauthorized_message: &str,
    ) -> Result<Response> {
        let response = request
            .send()
            .await
            .with_context(|| format!("Failed to call {}", call_name))?;

        if matches!(
            response.status(),
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN
        ) {
            anyhow::bail!("{unauthorized_message}");
        }

        response
            .error_for_status_ref()
            .with_context(|| format!("{} returned error", call_name))?;

        self.sync_mt_cookies_from_jar()?;
        Ok(response)
    }

    async fn get_json<T: DeserializeOwned>(
        &mut self,
        request: RequestBuilder,
        call_name: &str,
        unauthorized_message: &str,
    ) -> Result<T> {
        let response = self.send(request, call_name, unauthorized_message).await?;
        response
            .json::<T>()
            .await
            .with_context(|| format!("Failed to parse {} response", call_name))
    }

    async fn send_without_body(
        &mut self,
        request: RequestBuilder,
        call_name: &str,
        unauthorized_message: &str,
    ) -> Result<()> {
        let response = self.send(request, call_name, unauthorized_message).await?;
        let _ = response.bytes().await;
        Ok(())
    }

    pub async fn me(&mut self) -> Result<Me> {
        if self.dev_backend.is_some() {
            return Ok(Me {
                id: 1,
                email: "dev@localhost".to_string(),
                full_name: "Dev User".to_string(),
            });
        }

        self.get_json(
            self.client.get(self.endpoint("/me")?),
            "GET /me",
            UNAUTH_INVALID_SESSION,
        )
        .await
    }

    pub async fn get_active_timer(&mut self) -> Result<Option<ActiveTimerState>> {
        if self.dev_backend.is_some() {
            return Ok(None);
        }

        let response: GetTimerResponse = self
            .get_json(
                self.client.get(self.endpoint("/time-tracking/timer")?),
                "GET /time-tracking/timer",
                UNAUTH_RELOGIN,
            )
            .await?;

        Ok(response.timer)
    }

    pub async fn get_time_info(&mut self, from: time::Date, to: time::Date) -> Result<TimeInfo> {
        if let Some(dev) = &self.dev_backend {
            return Ok(dev.time_info());
        }

        let format = time::format_description::parse("[year]-[month]-[day]")?;
        let from_str = from.format(&format).context("Failed to format from date")?;
        let to_str = to.format(&format).context("Failed to format to date")?;

        self.get_json(
            self.client
                .get(self.endpoint("/time-tracking/time-info")?)
                .query(&[("from", &from_str), ("to", &to_str)]),
            "GET /time-tracking/time-info",
            UNAUTH_RELOGIN,
        )
        .await
    }

    pub async fn get_time_entries(
        &mut self,
        from: time::Date,
        to: time::Date,
    ) -> Result<Vec<TimeEntry>> {
        if let Some(dev) = &self.dev_backend {
            let from_str = format!(
                "{:04}-{:02}-{:02}",
                from.year(),
                from.month() as u8,
                from.day()
            );
            let to_str = format!("{:04}-{:02}-{:02}", to.year(), to.month() as u8, to.day());
            return Ok(dev
                .time_entries()
                .into_iter()
                .filter(|entry| entry.date >= from_str && entry.date <= to_str)
                .collect());
        }

        let format = time::format_description::parse("[year]-[month]-[day]")?;
        let from_str = from.format(&format).context("Failed to format from date")?;
        let to_str = to.format(&format).context("Failed to format to date")?;

        self.get_json(
            self.client
                .get(self.endpoint("/time-tracking/time-entries")?)
                .query(&[("from", &from_str), ("to", &to_str)]),
            "GET /time-tracking/time-entries",
            UNAUTH_RELOGIN,
        )
        .await
    }

    pub async fn start_timer(
        &mut self,
        project_id: Option<String>,
        project_name: Option<String>,
        activity_id: Option<String>,
        activity_name: Option<String>,
        note: Option<String>,
    ) -> Result<()> {
        if self.dev_backend.is_some() {
            return Ok(());
        }

        self.send_without_body(
            self.client
                .post(self.endpoint("/time-tracking/timer")?)
                .json(&StartTimerRequest {
                    project_id,
                    project_name,
                    activity_id,
                    activity_name,
                    user_note: note,
                }),
            "POST /time-tracking/timer",
            UNAUTH_RELOGIN,
        )
        .await
    }

    pub async fn save_timer(&mut self, note: Option<String>) -> Result<()> {
        if self.dev_backend.is_some() {
            return Ok(());
        }

        self.send_without_body(
            self.client
                .put(self.endpoint("/time-tracking/timer")?)
                .json(&SaveTimerRequest { user_note: note }),
            "PUT /time-tracking/timer",
            UNAUTH_RELOGIN,
        )
        .await
    }

    pub async fn stop_timer(&mut self) -> Result<()> {
        if self.dev_backend.is_some() {
            return Ok(());
        }

        self.send_without_body(
            self.client.delete(self.endpoint("/time-tracking/timer")?),
            "DELETE /time-tracking/timer",
            UNAUTH_RELOGIN,
        )
        .await
    }

    pub async fn update_active_timer(
        &mut self,
        project_id: Option<String>,
        project_name: Option<String>,
        activity_id: Option<String>,
        activity_name: Option<String>,
        note: Option<String>,
        start_time: Option<time::OffsetDateTime>,
    ) -> Result<()> {
        if self.dev_backend.is_some() {
            return Ok(());
        }

        self.send_without_body(
            self.client
                .put(self.endpoint("/time-tracking/update-timer")?)
                .json(&UpdateActiveTimerRequest {
                    project_id,
                    project_name,
                    activity_id,
                    activity_name,
                    user_note: note,
                    start_time,
                }),
            "PUT /time-tracking/update-timer",
            UNAUTH_RELOGIN,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
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
        if let Some(dev) = &self.dev_backend {
            dev.edit_entry(
                project_registration_id,
                project_id,
                project_name,
                activity_id,
                activity_name,
                start_time,
                end_time,
                user_note,
            );
            return Ok(());
        }

        let format = time::format_description::well_known::Rfc3339;
        let body = EditEntryRequest {
            project_registration_id,
            project_id,
            project_name,
            activity_id,
            activity_name,
            start_time: start_time
                .format(&format)
                .context("Failed to format start_time")?,
            end_time: end_time
                .format(&format)
                .context("Failed to format end_time")?,
            reg_day,
            week_number,
            user_note,
            original_reg_day: None,
            original_project_id,
            original_activity_id,
        };

        self.send_without_body(
            self.client
                .put(self.endpoint("/time-tracking/time-entries")?)
                .json(&body),
            "PUT /time-tracking/time-entries",
            UNAUTH_RELOGIN,
        )
        .await
    }

    pub async fn delete_time_entry(&mut self, registration_id: &str) -> Result<()> {
        if let Some(dev) = &self.dev_backend {
            dev.delete_entry(registration_id);
            return Ok(());
        }

        self.send_without_body(
            self.client
                .delete(self.endpoint("/time-tracking/time-entries")?)
                .json(&DeleteEntryRequest {
                    project_registration_id: registration_id,
                }),
            "DELETE /time-tracking/time-entries",
            UNAUTH_RELOGIN,
        )
        .await
    }

    pub async fn authenticate(&mut self, username: &str, password: &str) -> Result<()> {
        if self.dev_backend.is_some() {
            return Ok(());
        }

        self.send_without_body(
            self.client
                .post(self.endpoint("/time-tracking/authenticate")?)
                .json(&AuthenticateRequest { username, password }),
            "POST /time-tracking/authenticate",
            UNAUTH_INVALID_MILLTIME_CREDENTIALS,
        )
        .await
    }

    pub fn mt_cookies(&self) -> &[(String, String)] {
        &self.mt_cookies
    }

    pub async fn get_projects(&mut self) -> Result<Vec<Project>> {
        if let Some(dev) = &self.dev_backend {
            return Ok(dev.projects());
        }

        let dtos: Vec<ProjectDto> = self
            .get_json(
                self.client.get(self.endpoint("/time-tracking/projects")?),
                "GET /time-tracking/projects",
                UNAUTH_RELOGIN,
            )
            .await?;

        let mut projects: Vec<Project> = dtos
            .into_iter()
            .map(|dto| Project {
                id: dto.project_id,
                name: dto.project_name,
            })
            .collect();
        projects.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(projects)
    }

    pub async fn get_activities(&mut self, project_id: &str) -> Result<Vec<Activity>> {
        if let Some(dev) = &self.dev_backend {
            return Ok(dev.activities(project_id));
        }

        let dtos: Vec<ActivityDto> = self
            .get_json(
                self.client.get(self.endpoint(&format!(
                    "/time-tracking/projects/{}/activities",
                    project_id
                ))?),
                "GET /time-tracking/projects/:id/activities",
                UNAUTH_RELOGIN,
            )
            .await?;

        let mut activities: Vec<Activity> = dtos
            .into_iter()
            .map(|dto| Activity {
                id: dto.activity,
                name: dto.activity_name,
                project_id: project_id.to_string(),
            })
            .collect();

        activities.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(activities)
    }
}

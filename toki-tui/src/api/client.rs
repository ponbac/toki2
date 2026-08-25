use anyhow::{Context, Result};
use reqwest::{cookie::Jar, Client, RequestBuilder, Response, StatusCode, Url};
use serde::de::DeserializeOwned;
use std::sync::Arc;

use crate::api::dev_backend::DevBackend;
use crate::api::dto::{
    ActivityDto, EditEntryRequest, ProjectDto, SaveTimerRequest, StartTimerRequest,
    UpdateActiveTimerRequest,
};
use crate::types::{
    ActiveTimerState, Activity, GetTimerResponse, Me, Project, TimeEntry, TimeInfo,
};

const SESSION_COOKIE: &str = "id";
const UNAUTH_INVALID_SESSION: &str =
    "Session expired or invalid. Run `toki-tui login` to authenticate.";
const UNAUTH_RELOGIN: &str = "Session expired. Run `toki-tui login` to re-authenticate.";

#[derive(Debug, Clone)]
pub struct ApiClient {
    client: Client,
    base_url: Url,
    dev_backend: Option<DevBackend>,
}

impl ApiClient {
    pub fn new(base_url: &str, session_id: &str) -> Result<Self> {
        let base_url = Url::parse(base_url.trim_end_matches('/'))
            .with_context(|| format!("Invalid API URL: {}", base_url))?;
        let jar = Arc::new(Jar::default());

        jar.add_cookie_str(
            &format!("{}={}; Path=/", SESSION_COOKIE, session_id),
            &base_url,
        );

        let client = Client::builder()
            .cookie_provider(jar.clone())
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self {
            client,
            base_url,
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
            dev_backend: Some(DevBackend::new()),
        })
    }

    fn endpoint(&self, path: &str) -> Result<Url> {
        self.base_url
            .join(path)
            .with_context(|| format!("Failed to build URL for path {}", path))
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
        activity_id: Option<String>,
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
                    activity_id,
                    note,
                }),
            "POST /time-tracking/timer",
            UNAUTH_RELOGIN,
        )
        .await
    }

    pub async fn save_timer(&mut self, note: Option<String>, idempotency_key: &str) -> Result<()> {
        if self.dev_backend.is_some() {
            return Ok(());
        }

        self.send_without_body(
            self.client
                .post(self.endpoint("/time-tracking/timer/save")?)
                .header("Idempotency-Key", idempotency_key)
                .json(&SaveTimerRequest { note }),
            "POST /time-tracking/timer/save",
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

    pub async fn update_active_timer(&mut self, request: UpdateActiveTimerRequest) -> Result<()> {
        if self.dev_backend.is_some() {
            return Ok(());
        }

        self.send_without_body(
            self.client
                .patch(self.endpoint("/time-tracking/timer")?)
                .json(&request),
            "PATCH /time-tracking/timer",
            UNAUTH_RELOGIN,
        )
        .await
    }

    pub async fn edit_time_entry(
        &mut self,
        registration_id: &str,
        project_id: &str,
        activity_id: &str,
        start_time: time::OffsetDateTime,
        end_time: time::OffsetDateTime,
        note: &str,
    ) -> Result<()> {
        if let Some(dev) = &self.dev_backend {
            dev.edit_entry(
                registration_id,
                project_id,
                activity_id,
                start_time,
                end_time,
                note,
            );
            return Ok(());
        }

        let format = time::format_description::well_known::Rfc3339;
        let body = EditEntryRequest {
            project_id,
            activity_id,
            start_time: start_time
                .format(&format)
                .context("Failed to format start_time")?,
            end_time: end_time
                .format(&format)
                .context("Failed to format end_time")?,
            note,
        };

        self.send_without_body(
            self.client
                .put(self.endpoint(&format!(
                    "/time-tracking/time-entries/{}",
                    urlencoding::encode(registration_id)
                ))?)
                .json(&body),
            "PUT /time-tracking/time-entries/:id",
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
            self.client.delete(self.endpoint(&format!(
                "/time-tracking/time-entries/{}",
                urlencoding::encode(registration_id)
            ))?),
            "DELETE /time-tracking/time-entries/:id",
            UNAUTH_RELOGIN,
        )
        .await
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

#[cfg(test)]
mod tests {
    use super::*;
    use time::macros::datetime;
    use tokio::{
        io::{AsyncReadExt, AsyncWriteExt},
        net::TcpListener,
        task::JoinHandle,
    };

    async fn capturing_client(status: &'static str) -> (ApiClient, JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
        let address = listener.local_addr().expect("listener address");
        let capture = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut request = Vec::new();
            let mut chunk = [0; 4096];

            loop {
                let read = socket.read(&mut chunk).await.expect("read request");
                assert!(read > 0, "connection closed before request completed");
                request.extend_from_slice(&chunk[..read]);

                let Some(header_end) = request.windows(4).position(|part| part == b"\r\n\r\n")
                else {
                    continue;
                };
                let headers = String::from_utf8_lossy(&request[..header_end]).to_lowercase();
                let content_length = headers
                    .lines()
                    .find_map(|line| line.strip_prefix("content-length: "))
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or_default();
                if request.len() >= header_end + 4 + content_length {
                    break;
                }
            }

            let response =
                format!("HTTP/1.1 {status}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write response");
            String::from_utf8(request).expect("HTTP request is UTF-8")
        });

        (
            ApiClient::new(&format!("http://{address}"), "test-session").expect("client"),
            capture,
        )
    }

    fn json_body(request: &str) -> serde_json::Value {
        let (_, body) = request.split_once("\r\n\r\n").expect("request body");
        serde_json::from_str(body).expect("JSON body")
    }

    #[tokio::test]
    async fn timer_write_requests_match_the_http_contract() {
        let (mut client, capture) = capturing_client("201 Created").await;
        client
            .start_timer(
                Some("project/1".into()),
                Some("activity-1".into()),
                Some("Ship it".into()),
            )
            .await
            .expect("start timer");
        let request = capture.await.expect("capture");
        assert!(request.starts_with("POST /time-tracking/timer HTTP/1.1\r\n"));
        assert_eq!(
            json_body(&request),
            serde_json::json!({
                "projectId": "project/1",
                "activityId": "activity-1",
                "note": "Ship it"
            })
        );

        let (mut client, capture) = capturing_client("201 Created").await;
        client
            .save_timer(Some("Done".into()), "save-key-1")
            .await
            .expect("save timer");
        let request = capture.await.expect("capture");
        assert!(request.starts_with("POST /time-tracking/timer/save HTTP/1.1\r\n"));
        assert!(request
            .to_lowercase()
            .contains("\r\nidempotency-key: save-key-1\r\n"));
        assert_eq!(json_body(&request), serde_json::json!({"note": "Done"}));

        let (mut client, capture) = capturing_client("200 OK").await;
        client
            .update_active_timer(UpdateActiveTimerRequest::note("Changed".into()))
            .await
            .expect("patch timer");
        let request = capture.await.expect("capture");
        assert!(request.starts_with("PATCH /time-tracking/timer HTTP/1.1\r\n"));
        assert_eq!(json_body(&request), serde_json::json!({"note": "Changed"}));

        let (mut client, capture) = capturing_client("200 OK").await;
        client
            .edit_time_entry(
                "entry/1",
                "project-1",
                "activity-1",
                datetime!(2026-08-25 08:00 UTC),
                datetime!(2026-08-25 09:30 UTC),
                "Reviewed",
            )
            .await
            .expect("edit entry");
        let request = capture.await.expect("capture");
        assert!(request.starts_with("PUT /time-tracking/time-entries/entry%2F1 HTTP/1.1\r\n"));
        assert_eq!(
            json_body(&request),
            serde_json::json!({
                "projectId": "project-1",
                "activityId": "activity-1",
                "startTime": "2026-08-25T08:00:00Z",
                "endTime": "2026-08-25T09:30:00Z",
                "note": "Reviewed"
            })
        );

        let (mut client, capture) = capturing_client("204 No Content").await;
        client
            .delete_time_entry("entry/1")
            .await
            .expect("delete entry");
        let request = capture.await.expect("capture");
        assert!(request.starts_with("DELETE /time-tracking/time-entries/entry%2F1 HTTP/1.1\r\n"));
        assert!(request.ends_with("\r\n\r\n"));
    }

    #[test]
    fn timer_selection_patch_serializes_explicit_clears() {
        let request = UpdateActiveTimerRequest::selection(None, None);
        assert_eq!(
            serde_json::to_value(request).expect("serialize patch"),
            serde_json::json!({"projectId": null, "activityId": null})
        );
    }
}

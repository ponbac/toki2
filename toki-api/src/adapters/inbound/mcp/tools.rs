//! The MCP tools: the curated agent surface of `/openapi.json`, as typed tools.
//!
//! Each tool calls the same handler as its OpenAPI operation, so access checks,
//! validation and provider rules stay in one place. Arguments reach a handler
//! through serde, in the wire shape its request type already reads.

use std::{marker::PhantomData, sync::Arc};

use axum::{
    extract::{Path, Query, State},
    http::request::Parts,
    Json,
};
use axum_extra::extract::WithRejection;
use rmcp::{
    handler::server::{
        common::Extension,
        tool::{InputResponses, RequestState},
        wrapper::Parameters,
    },
    model::{
        CallToolResponse, CallToolResult, ContentBlock, ErrorData, Implementation,
        ServerCapabilities, ServerConfig,
    },
    schemars::JsonSchema,
    service::RequestContext,
    tool, tool_handler, tool_router, RoleServer, ServerHandler,
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::confirm::{Confirmation, Confirmations, PriorRound};
use crate::{
    adapters::inbound::http::{ai_subscriptions, ai_usage_reports, TimerResponse},
    app_state::AppState,
    auth::AuthUser,
    routes::{
        pull_requests,
        time_tracking::{calendar, connection, projects, timer},
        work_items, ApiError,
    },
};

const INSTRUCTIONS: &str = "\
Toki is the user's work-time tracker. It books time in the company's time-tracking \
provider, shows Azure DevOps work items and the pull requests the user follows, and \
reports the user's own AI usage. Every tool acts as the signed-in user only.

- Dates are `YYYY-MM-DD`; timestamps are RFC 3339 with an offset.
- Before starting a timer or creating a time entry, look up IDs with \
`list_time_tracking_projects` and `list_time_tracking_activities`, and pass each ID with its name.
- `save_active_timer` books the running timer as a time entry. `discard_active_timer` \
deletes the timer without booking anything.
- Time-entry notes for work items conventionally read `#<parentId> #<workItemId> - <title>`, \
with `[CR] ` before the title for code review. Read `get_work_item_context` to write one.";

#[derive(Clone)]
pub struct TokiMcp {
    app_state: AppState,
    confirmations: Arc<Confirmations>,
}

impl TokiMcp {
    pub(super) fn new(app_state: AppState, confirmations: Arc<Confirmations>) -> Self {
        Self {
            app_state,
            confirmations,
        }
    }

    fn state(&self) -> State<AppState> {
        State(self.app_state.clone())
    }

    fn service<T: Clone>(&self, service: &T) -> State<T> {
        State(service.clone())
    }

    /// Asks the user before a destructive call; `None` means go ahead.
    async fn confirmation(
        &self,
        context: &RequestContext<RoleServer>,
        prior: PriorRound,
        user: &AuthUser,
        tool: &str,
        arguments: &impl Serialize,
        message: String,
    ) -> Result<Option<CallToolResponse>, ErrorData> {
        match self
            .confirmations
            .confirm(context, prior, user, tool, arguments, message)
            .await?
        {
            Confirmation::Proceed => Ok(None),
            // A client can answer for the user, by its approval settings, so
            // the result does not claim the user saw the confirmation.
            Confirmation::Declined => Ok(Some(
                CallToolResult::success(vec![ContentBlock::text(
                    "The confirmation was declined, by the user or by the client's \
                     approval settings. Nothing was changed.",
                )])
                .into(),
            )),
            Confirmation::Cancelled => Ok(Some(
                CallToolResult::success(vec![ContentBlock::text(
                    "The confirmation was dismissed without an answer. Nothing was changed.",
                )])
                .into(),
            )),
            Confirmation::Ask(input_required) => {
                Ok(Some(CallToolResponse::InputRequired(input_required)))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Arguments
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DateRangeArgs {
    /// Inclusive range start, `YYYY-MM-DD`.
    from: String,
    /// Inclusive range end, `YYYY-MM-DD`.
    to: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct FlexBalanceArgs {
    /// Last day to estimate flex through, `YYYY-MM-DD`. Normally today.
    through: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct TimeEntriesArgs {
    /// Inclusive range start, `YYYY-MM-DD`.
    from: String,
    /// Inclusive range end, `YYYY-MM-DD`.
    to: String,
    /// Collapse entries with the same project, activity and note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    unique: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectIdArgs {
    /// Time-tracking project ID, from `list_time_tracking_projects`.
    project_id: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct StartTimerArgs {
    /// What the time is for.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user_note: Option<String>,
    /// Project ID. Provide together with `projectName`, or omit both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
    /// Project name. Provide together with `projectId`, or omit both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_name: Option<String>,
    /// Activity ID. Provide together with `activityName`, or omit both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    activity_id: Option<String>,
    /// Activity name. Provide together with `activityId`, or omit both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    activity_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTimerArgs {
    /// A replacement note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    activity_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    activity_name: Option<String>,
    /// A corrected start, RFC 3339.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    start_time: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SaveTimerArgs {
    /// A replacement note for the booked entry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user_note: Option<String>,
    /// Start a new timer right after booking this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    restart_timer: Option<RestartTimerArgs>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RestartTimerArgs {
    user_note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    activity_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    activity_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateTimeEntryArgs {
    project_id: String,
    project_name: String,
    activity_id: String,
    activity_name: String,
    /// Interval start, RFC 3339.
    start_time: String,
    /// Interval end, RFC 3339.
    end_time: String,
    user_note: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTimeEntryArgs {
    /// The entry's `registrationId`, from `list_time_entries`.
    project_registration_id: String,
    project_id: String,
    project_name: String,
    activity_id: String,
    activity_name: String,
    /// Interval start, RFC 3339.
    start_time: String,
    /// Interval end, RFC 3339.
    end_time: String,
    user_note: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeleteTimeEntryArgs {
    /// The entry's `registrationId`, from `list_time_entries`.
    project_registration_id: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemProjectArgs {
    /// Organization, from `list_work_item_projects`.
    organization: String,
    /// Project, from `list_work_item_projects`.
    project: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BoardArgs {
    organization: String,
    project: String,
    /// Iteration path, from `list_work_item_iterations`. Defaults to the current one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    iteration_path: Option<String>,
    /// Team whose board to read. Defaults to the project's default team.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    team: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct WorkItemArgs {
    organization: String,
    project: String,
    work_item_id: String,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MoveWorkItemArgs {
    organization: String,
    project: String,
    work_item_id: String,
    /// The board column to move the item to, as named on the board.
    target_column_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    iteration_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    team: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AiProvider {
    Codex,
    Claude,
    Grok,
    Copilot,
    Pi,
    OpenCode,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AiSessionSort {
    Recent,
    Cost,
    Tokens,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageReportArgs {
    /// First local day, `YYYY-MM-DD`. Defaults to the first day of `to`'s month.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    /// Last local day, inclusive. Defaults to the end of this month. At most 366 days.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    provider: Option<AiProvider>,
    /// Only this model's usage; empty for usage of an unknown model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
    /// Only usage attributed to this time-tracking project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
    /// Only usage attributed to no project.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    unassigned: bool,
    /// Only usage of project keys marked personal.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    personal: bool,
    /// Only this machine's usage.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    machine_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiUsageSessionArgs {
    #[serde(flatten)]
    report: AiUsageReportArgs,
    /// `recent` (default), `cost` or `tokens`, largest first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    sort: Option<AiSessionSort>,
    /// Sessions per page, 1 to 1000. Defaults to 100.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    limit: Option<usize>,
    /// A previous page's `nextCursor`, with the same sort, range and filters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    after: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OptionalDateRangeArgs {
    /// First local day, `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    from: Option<String>,
    /// Last local day, inclusive.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    to: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiSubscriptionArgs {
    /// Not `pi`: a ChatGPT subscription declared for `codex` also covers Pi.
    /// Not `opencode`: usage it routes through Copilot or a ChatGPT sign-in is
    /// billed to that subscription (`copilot` or `codex`), the rest at API cost.
    provider: AiProvider,
    /// Plan name, such as `Claude Max 5x`. At most 64 characters.
    plan: String,
    /// Monthly fee as a decimal string with at most two decimals, such as `1100.00`.
    monthly_cost: String,
    /// ISO 4217 currency code, such as `SEK`.
    currency: String,
    /// First covered day, `YYYY-MM-DD`.
    valid_from: String,
    /// Last covered day, inclusive. Omit while ongoing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    valid_to: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAiSubscriptionArgs {
    /// Subscription ID, from `list_ai_subscriptions`.
    subscription_id: i32,
    #[serde(flatten)]
    terms: AiSubscriptionArgs,
}

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AiSubscriptionIdArgs {
    /// Subscription ID, from `list_ai_subscriptions`.
    subscription_id: i32,
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

#[tool_router]
impl TokiMcp {
    // Time tracking ----------------------------------------------------------

    /// Whether the user is mapped to a time-tracking provider user, and as whom.
    #[tool(annotations(title = "Get time-tracking connection", read_only_hint = true))]
    async fn get_time_tracking_connection(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(connection::connection_status(user, self.state()).await)
    }

    /// Projects the user can book time on.
    #[tool(annotations(title = "List time-tracking projects", read_only_hint = true))]
    async fn list_time_tracking_projects(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(projects::list_projects(self.state(), user).await)
    }

    /// Activities the user can book today on one project.
    #[tool(annotations(title = "List time-tracking activities", read_only_hint = true))]
    async fn list_time_tracking_activities(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<ProjectIdArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(projects::list_activities(Path(args.project_id), self.state(), user).await)
    }

    /// Worked, scheduled and remaining hours in a date range.
    #[tool(annotations(title = "Get time info", read_only_hint = true))]
    async fn get_time_info(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<DateRangeArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(calendar::get_time_info(user, self.state(), Query(convert(&args)?)).await)
    }

    /// The user's preliminary flex balance in hours: the balance the provider has
    /// settled through `settledThrough`, plus the estimated flex of the days
    /// after it through `through`.
    #[tool(annotations(title = "Get flex balance", read_only_hint = true))]
    async fn get_flex_balance(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<FlexBalanceArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(calendar::get_flex_balance(user, self.state(), Query(convert(&args)?)).await)
    }

    /// Booked time entries in a date range, with their `registrationId`s.
    #[tool(annotations(title = "List time entries", read_only_hint = true))]
    async fn list_time_entries(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<TimeEntriesArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(calendar::get_time_entries(user, self.state(), Query(convert(&args)?)).await)
    }

    /// Each day's attestation status in a date range. Attested days are locked.
    #[tool(annotations(title = "List time-entry day statuses", read_only_hint = true))]
    async fn list_time_entry_day_statuses(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<DateRangeArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(
            calendar::get_time_entry_day_statuses(user, self.state(), Query(convert(&args)?)).await,
        )
    }

    /// The user's running timer, or `null` when none runs.
    #[tool(annotations(title = "Get active timer", read_only_hint = true))]
    async fn get_active_timer(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(timer::get_timer(user, self.state()).await)
    }

    /// Recent timer runs, booked or not.
    #[tool(annotations(title = "List timer history", read_only_hint = true))]
    async fn list_timer_history(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(timer::get_timer_history(user, self.state()).await)
    }

    /// Starts a timer now. Fails when one already runs.
    #[tool(annotations(
        title = "Start timer",
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = false
    ))]
    async fn start_timer(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<StartTimerArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        if let Err(error) =
            timer::start_timer(user.clone(), self.state(), Json(convert(&args)?)).await
        {
            return Ok(tool_error(error));
        }
        respond(timer::get_timer(user, self.state()).await)
    }

    /// Changes the running timer's note, project, activity or start. Omitted
    /// fields keep their values.
    #[tool(annotations(
        title = "Update active timer",
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = true
    ))]
    async fn update_active_timer(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<UpdateTimerArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        if let Err(error) =
            timer::edit_timer(user.clone(), self.state(), Json(convert(&args)?)).await
        {
            return Ok(tool_error(error));
        }
        respond(timer::get_timer(user, self.state()).await)
    }

    /// Books the running timer as a time entry, ending now, and can start a
    /// new timer right after.
    #[tool(annotations(
        title = "Save active timer",
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = false
    ))]
    async fn save_active_timer(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<SaveTimerArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(timer::save_timer(user, self.state(), Json(convert(&args)?)).await)
    }

    /// Deletes the running timer without booking any time.
    #[tool(annotations(
        title = "Discard active timer",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = false
    ))]
    async fn discard_active_timer(
        &self,
        Extension(parts): Extension<Parts>,
        context: RequestContext<RoleServer>,
        RequestState(request_state): RequestState,
        InputResponses(input_responses): InputResponses,
    ) -> Result<CallToolResponse, ErrorData> {
        let user = authenticated(&parts)?;
        let running = match timer::get_timer(user.clone(), self.state()).await {
            Ok(Json(response)) => response.timer,
            Err(error) => return Ok(tool_error(error).into()),
        };
        let Some(running) = running else {
            return Ok(tool_error(ApiError::not_found("No timer is running.")).into());
        };
        let message = format!(
            "Discard the running timer without booking it? {}",
            describe_timer(&running)
        );
        if let Some(response) = self
            .confirmation(
                &context,
                PriorRound {
                    request_state,
                    input_responses,
                },
                &user,
                "discard_active_timer",
                &running.start_time.unix_timestamp(),
                message,
            )
            .await?
        {
            return Ok(response);
        }
        Ok(respond_done(
            timer::stop_timer(user, self.state()).await,
            "Timer discarded.",
        )
        .into())
    }

    /// Books a time entry for a past or future interval, without the timer.
    #[tool(annotations(
        title = "Create time entry",
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = false
    ))]
    async fn create_time_entry(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<CreateTimeEntryArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(
            calendar::create_project_registration(user, self.state(), Json(convert(&args)?))
                .await
                .map(|(_, body)| body),
        )
    }

    /// Replaces a booked time entry's project, activity, interval and note.
    #[tool(annotations(
        title = "Update time entry",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = true
    ))]
    async fn update_time_entry(
        &self,
        Extension(parts): Extension<Parts>,
        context: RequestContext<RoleServer>,
        RequestState(request_state): RequestState,
        InputResponses(input_responses): InputResponses,
        Parameters(args): Parameters<UpdateTimeEntryArgs>,
    ) -> Result<CallToolResponse, ErrorData> {
        let user = authenticated(&parts)?;
        let message = format!(
            "Change time entry {} to {} / {}, {} – {}, note \"{}\"?",
            args.project_registration_id,
            args.project_name,
            args.activity_name,
            args.start_time,
            args.end_time,
            args.user_note
        );
        if let Some(response) = self
            .confirmation(
                &context,
                PriorRound {
                    request_state,
                    input_responses,
                },
                &user,
                "update_time_entry",
                &args,
                message,
            )
            .await?
        {
            return Ok(response);
        }
        Ok(respond(
            calendar::edit_project_registration(user, self.state(), Json(convert(&args)?)).await,
        )?
        .into())
    }

    /// Deletes a booked time entry.
    #[tool(annotations(
        title = "Delete time entry",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = true
    ))]
    async fn delete_time_entry(
        &self,
        Extension(parts): Extension<Parts>,
        context: RequestContext<RoleServer>,
        RequestState(request_state): RequestState,
        InputResponses(input_responses): InputResponses,
        Parameters(args): Parameters<DeleteTimeEntryArgs>,
    ) -> Result<CallToolResponse, ErrorData> {
        let user = authenticated(&parts)?;
        let message = format!("Delete time entry {}?", args.project_registration_id);
        if let Some(response) = self
            .confirmation(
                &context,
                PriorRound {
                    request_state,
                    input_responses,
                },
                &user,
                "delete_time_entry",
                &args,
                message,
            )
            .await?
        {
            return Ok(response);
        }
        Ok(respond_done(
            calendar::delete_project_registration(user, self.state(), Json(convert(&args)?)).await,
            "Time entry deleted.",
        )
        .into())
    }

    // Work items -------------------------------------------------------------

    /// Organization/project pairs whose work items the user can read.
    #[tool(annotations(title = "List work-item projects", read_only_hint = true))]
    async fn list_work_item_projects(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(work_items::get_projects(user, self.state()).await)
    }

    /// Sprints of one work-item project.
    #[tool(annotations(title = "List work-item iterations", read_only_hint = true))]
    async fn list_work_item_iterations(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<WorkItemProjectArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(work_items::get_iterations(user, self.state(), Query(convert(&args)?)).await)
    }

    /// Board columns and work items of a project iteration, with linked pull requests.
    #[tool(annotations(title = "Get work-item board", read_only_hint = true))]
    async fn get_work_item_board(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<BoardArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(work_items::get_board(user, self.state(), Query(convert(&args)?)).await)
    }

    /// One work item as Markdown: title, description, acceptance criteria,
    /// comments and parent. Use it to understand the work or write a note.
    #[tool(annotations(title = "Get work-item context", read_only_hint = true))]
    async fn get_work_item_context(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<WorkItemArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        match work_items::format_for_llm(user, self.state(), Query(convert(&args)?)).await {
            Ok(Json(response)) => {
                let markdown = response.markdown.clone();
                let mut result = structured(&response)?;
                result.content = vec![ContentBlock::text(markdown)];
                Ok(result)
            }
            Err(error) => Ok(tool_error(error)),
        }
    }

    /// Moves a work item to another column of its board.
    #[tool(annotations(
        title = "Move work item",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = true
    ))]
    async fn move_work_item(
        &self,
        Extension(parts): Extension<Parts>,
        context: RequestContext<RoleServer>,
        RequestState(request_state): RequestState,
        InputResponses(input_responses): InputResponses,
        Parameters(args): Parameters<MoveWorkItemArgs>,
    ) -> Result<CallToolResponse, ErrorData> {
        let user = authenticated(&parts)?;
        let message = format!(
            "Move work item #{} in {}/{} to \"{}\"?",
            args.work_item_id, args.organization, args.project, args.target_column_name
        );
        if let Some(response) = self
            .confirmation(
                &context,
                PriorRound {
                    request_state,
                    input_responses,
                },
                &user,
                "move_work_item",
                &args,
                message,
            )
            .await?
        {
            return Ok(response);
        }
        Ok(respond_done(
            work_items::move_work_item(user, self.state(), Json(convert(&args)?)).await,
            "Work item moved.",
        )
        .into())
    }

    // Pull requests ----------------------------------------------------------

    /// Open pull requests in repositories the user follows, newest first, with
    /// reviewers, threads and linked work items.
    #[tool(annotations(title = "List pull requests", read_only_hint = true))]
    async fn list_pull_requests(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(pull_requests::list_pull_requests(user, self.state()).await)
    }

    // AI usage ---------------------------------------------------------------

    /// The user's AI usage totals, cost and breakdowns for a range of local days.
    #[tool(annotations(title = "Get AI usage report", read_only_hint = true))]
    async fn get_ai_usage_report(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<AiUsageReportArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(
            ai_usage_reports::get_report(
                user,
                self.service(&self.app_state.ai_usage_report_service),
                WithRejection(Query(convert(&args)?), PhantomData),
            )
            .await,
        )
    }

    /// The user's AI sessions with their usage, paged.
    #[tool(annotations(title = "List AI usage sessions", read_only_hint = true))]
    async fn list_ai_usage_sessions(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<AiUsageSessionArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(
            ai_usage_reports::list_sessions(
                user,
                self.service(&self.app_state.ai_usage_report_service),
                WithRejection(Query(convert(&args)?), PhantomData),
            )
            .await,
        )
    }

    /// The user's machines that upload AI usage, with each one's usage in a range.
    #[tool(annotations(title = "List AI usage machines", read_only_hint = true))]
    async fn list_ai_usage_machines(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<OptionalDateRangeArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(
            ai_usage_reports::list_machines(
                user,
                self.service(&self.app_state.ai_usage_report_service),
                WithRejection(Query(convert(&args)?), PhantomData),
            )
            .await,
        )
    }

    /// The project keys seen in the user's AI usage, and how each maps to a project.
    #[tool(annotations(title = "List AI usage project keys", read_only_hint = true))]
    async fn list_ai_usage_project_keys(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(
            ai_usage_reports::list_project_keys(
                user,
                self.service(&self.app_state.ai_usage_report_service),
            )
            .await,
        )
    }

    /// The AI subscriptions the user declared.
    #[tool(annotations(title = "List AI subscriptions", read_only_hint = true))]
    async fn list_ai_subscriptions(
        &self,
        Extension(parts): Extension<Parts>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(
            ai_subscriptions::list_subscriptions(
                user,
                self.service(&self.app_state.ai_subscription_service),
            )
            .await,
        )
    }

    /// Days whose AI usage does not match the declared subscriptions. Defaults
    /// to the last 90 days.
    #[tool(annotations(title = "List AI subscription mismatches", read_only_hint = true))]
    async fn list_ai_subscription_mismatches(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<OptionalDateRangeArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(
            ai_subscriptions::list_subscription_mismatches(
                user,
                self.service(&self.app_state.ai_subscription_service),
                WithRejection(Query(convert(&args)?), PhantomData),
            )
            .await,
        )
    }

    /// Declares an AI subscription the user pays for.
    #[tool(annotations(
        title = "Create AI subscription",
        read_only_hint = false,
        destructive_hint = false,
        idempotent_hint = false
    ))]
    async fn create_ai_subscription(
        &self,
        Extension(parts): Extension<Parts>,
        Parameters(args): Parameters<AiSubscriptionArgs>,
    ) -> Result<CallToolResult, ErrorData> {
        let user = authenticated(&parts)?;
        respond(
            ai_subscriptions::create_subscription(
                user,
                self.service(&self.app_state.ai_subscription_service),
                WithRejection(Json(convert(&args)?), PhantomData),
            )
            .await
            .map(|(_, body)| body),
        )
    }

    /// Replaces an AI subscription's terms.
    #[tool(annotations(
        title = "Update AI subscription",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = true
    ))]
    async fn update_ai_subscription(
        &self,
        Extension(parts): Extension<Parts>,
        context: RequestContext<RoleServer>,
        RequestState(request_state): RequestState,
        InputResponses(input_responses): InputResponses,
        Parameters(args): Parameters<UpdateAiSubscriptionArgs>,
    ) -> Result<CallToolResponse, ErrorData> {
        let user = authenticated(&parts)?;
        let terms = &args.terms;
        let message = format!(
            "Change AI subscription {} to {} {} at {} {} a month, from {}{}?",
            args.subscription_id,
            serde_json::to_value(&terms.provider)
                .ok()
                .and_then(|value| value.as_str().map(str::to_owned))
                .unwrap_or_default(),
            terms.plan,
            terms.monthly_cost,
            terms.currency,
            terms.valid_from,
            terms
                .valid_to
                .as_deref()
                .map(|to| format!(" to {to}"))
                .unwrap_or_default()
        );
        if let Some(response) = self
            .confirmation(
                &context,
                PriorRound {
                    request_state,
                    input_responses,
                },
                &user,
                "update_ai_subscription",
                &args,
                message,
            )
            .await?
        {
            return Ok(response);
        }
        Ok(respond(
            ai_subscriptions::update_subscription(
                user,
                self.service(&self.app_state.ai_subscription_service),
                WithRejection(Path(args.subscription_id), PhantomData),
                WithRejection(Json(convert(&args.terms)?), PhantomData),
            )
            .await,
        )?
        .into())
    }

    /// Deletes a declared AI subscription.
    #[tool(annotations(
        title = "Delete AI subscription",
        read_only_hint = false,
        destructive_hint = true,
        idempotent_hint = true
    ))]
    async fn delete_ai_subscription(
        &self,
        Extension(parts): Extension<Parts>,
        context: RequestContext<RoleServer>,
        RequestState(request_state): RequestState,
        InputResponses(input_responses): InputResponses,
        Parameters(args): Parameters<AiSubscriptionIdArgs>,
    ) -> Result<CallToolResponse, ErrorData> {
        let user = authenticated(&parts)?;
        let message = format!("Delete AI subscription {}?", args.subscription_id);
        if let Some(response) = self
            .confirmation(
                &context,
                PriorRound {
                    request_state,
                    input_responses,
                },
                &user,
                "delete_ai_subscription",
                &args,
                message,
            )
            .await?
        {
            return Ok(response);
        }
        Ok(respond_done(
            ai_subscriptions::delete_subscription(
                user,
                self.service(&self.app_state.ai_subscription_service),
                WithRejection(Path(args.subscription_id), PhantomData),
            )
            .await,
            "AI subscription deleted.",
        )
        .into())
    }
}

#[tool_handler]
impl ServerHandler for TokiMcp {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("toki", env!("CARGO_PKG_VERSION")))
            .with_instructions(INSTRUCTIONS)
    }
}

/// The caller the MCP authentication middleware resolved.
fn authenticated(parts: &Parts) -> Result<AuthUser, ErrorData> {
    parts
        .extensions
        .get::<AuthUser>()
        .cloned()
        .ok_or_else(|| ErrorData::internal_error("The request is not authenticated.", None))
}

/// Re-reads tool arguments as the handler's request type.
fn convert<T: DeserializeOwned>(args: &impl Serialize) -> Result<T, ErrorData> {
    serde_json::to_value(args)
        .and_then(serde_json::from_value)
        .map_err(|error| ErrorData::invalid_params(error.to_string(), None))
}

fn structured(value: &impl Serialize) -> Result<CallToolResult, ErrorData> {
    let value = serde_json::to_value(value)
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
    Ok(CallToolResult::structured(value))
}

fn respond<T: Serialize>(result: Result<Json<T>, ApiError>) -> Result<CallToolResult, ErrorData> {
    match result {
        Ok(Json(body)) => structured(&body),
        Err(error) => Ok(tool_error(error)),
    }
}

fn respond_done<T>(result: Result<T, ApiError>, done: &str) -> CallToolResult {
    match result {
        Ok(_) => CallToolResult::success(vec![ContentBlock::text(done)]),
        Err(error) => tool_error(error),
    }
}

/// A failed operation, reported to the model so it can correct course.
fn tool_error(error: ApiError) -> CallToolResult {
    CallToolResult::error(vec![ContentBlock::text(format!(
        "{} ({})",
        error.message(),
        error.status()
    ))])
}

fn describe_timer(timer: &TimerResponse) -> String {
    let mut parts = Vec::new();
    if let Some(project) = &timer.project_name {
        parts.push(project.clone());
    }
    if let Some(activity) = &timer.activity_name {
        parts.push(activity.clone());
    }
    parts.push(format!("running {}h {}m", timer.hours, timer.minutes));
    if !timer.note.is_empty() {
        parts.push(format!("note \"{}\"", timer.note));
    }
    parts.join(", ")
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;
    use crate::adapters::inbound::http::agent_openapi;

    /// Each agent OpenAPI operation and the tool that exposes it.
    const OPERATION_TOOLS: &[(&str, &str)] = &[
        ("getTimeTrackingConnection", "get_time_tracking_connection"),
        ("listTimeTrackingProjects", "list_time_tracking_projects"),
        (
            "listTimeTrackingActivities",
            "list_time_tracking_activities",
        ),
        ("getTimeInfo", "get_time_info"),
        ("getFlexBalance", "get_flex_balance"),
        ("listTimeEntries", "list_time_entries"),
        ("listTimeEntryDayStatuses", "list_time_entry_day_statuses"),
        ("getActiveTimer", "get_active_timer"),
        ("listTimerHistory", "list_timer_history"),
        ("startActiveTimer", "start_timer"),
        ("updateActiveTimer", "update_active_timer"),
        ("saveActiveTimer", "save_active_timer"),
        ("stopActiveTimer", "discard_active_timer"),
        ("createTimeEntry", "create_time_entry"),
        ("updateTimeEntry", "update_time_entry"),
        ("deleteTimeEntry", "delete_time_entry"),
        ("listWorkItemProjects", "list_work_item_projects"),
        ("listWorkItemIterations", "list_work_item_iterations"),
        ("getWorkItemBoard", "get_work_item_board"),
        ("formatWorkItemForLlm", "get_work_item_context"),
        ("moveWorkItem", "move_work_item"),
        ("listPullRequests", "list_pull_requests"),
        ("getAiUsageReport", "get_ai_usage_report"),
        ("listAiUsageSessions", "list_ai_usage_sessions"),
        ("listAiUsageMachines", "list_ai_usage_machines"),
        ("listAiUsageProjectKeys", "list_ai_usage_project_keys"),
        ("listAiSubscriptions", "list_ai_subscriptions"),
        (
            "listAiSubscriptionMismatches",
            "list_ai_subscription_mismatches",
        ),
        ("createAiSubscription", "create_ai_subscription"),
        ("updateAiSubscription", "update_ai_subscription"),
        ("deleteAiSubscription", "delete_ai_subscription"),
    ];

    /// Operations deliberately left out: machine usage uploads are a sync
    /// protocol for usage collectors, not an agent action.
    const OPERATIONS_WITHOUT_TOOLS: &[&str] = &["uploadAiUsage"];

    /// Tools that ask the user to confirm before they run.
    const CONFIRMED_TOOLS: &[&str] = &[
        "discard_active_timer",
        "update_time_entry",
        "delete_time_entry",
        "move_work_item",
        "update_ai_subscription",
        "delete_ai_subscription",
    ];

    /// HTTP method of each agent operation, by operation ID.
    fn operation_methods() -> BTreeMap<String, String> {
        let spec = serde_json::to_value(agent_openapi()).unwrap();
        let mut methods = BTreeMap::new();
        for item in spec["paths"].as_object().unwrap().values() {
            for (method, operation) in item.as_object().unwrap() {
                if let Some(id) = operation["operationId"].as_str() {
                    methods.insert(id.to_string(), method.clone());
                }
            }
        }
        methods
    }

    #[test]
    fn every_agent_operation_has_exactly_one_tool() {
        let operations: BTreeSet<_> = operation_methods().into_keys().collect();
        let covered: BTreeSet<_> = OPERATION_TOOLS
            .iter()
            .map(|(operation, _)| operation.to_string())
            .chain(OPERATIONS_WITHOUT_TOOLS.iter().map(|op| op.to_string()))
            .collect();
        assert_eq!(operations, covered);

        let tools: BTreeSet<_> = TokiMcp::tool_router()
            .list_all()
            .into_iter()
            .map(|tool| tool.name.to_string())
            .collect();
        let expected: BTreeSet<_> = OPERATION_TOOLS
            .iter()
            .map(|(_, tool)| tool.to_string())
            .collect();
        assert_eq!(tools, expected);
    }

    #[test]
    fn annotations_follow_each_operation() {
        let methods = operation_methods();
        let tools: BTreeMap<_, _> = TokiMcp::tool_router()
            .list_all()
            .into_iter()
            .map(|tool| (tool.name.to_string(), tool))
            .collect();

        for (operation, name) in OPERATION_TOOLS {
            let tool = &tools[*name];
            let annotations = tool.annotations.as_ref().expect("every tool is annotated");
            assert!(annotations.title.is_some(), "{name} has a title");
            assert!(tool.description.is_some(), "{name} has a description");

            let read_only = methods[*operation] == "get";
            assert_eq!(annotations.read_only_hint, Some(read_only), "{name}");
            if !read_only {
                let confirmed = CONFIRMED_TOOLS.contains(name);
                assert_eq!(annotations.destructive_hint, Some(confirmed), "{name}");
                assert!(annotations.idempotent_hint.is_some(), "{name}");
            }
        }
    }
}

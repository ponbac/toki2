//! Human confirmation for destructive tools, through MCP elicitation.
//!
//! Clients on protocol `2026-07-28` and later get a multi round-trip request:
//! the tool returns an elicitation and a sealed `requestState`, and the client
//! retries with the user's answer. Older clients get a server-to-client
//! elicitation request within their session. A client that supports neither
//! runs the tool directly: its own approval of destructive tools, which tool
//! annotations announce, is then the only confirmation.
//!
//! Clients known to show a form without fields as an approval prompt get one;
//! accepting it confirms. Others get one required checkbox, as VS Code shows
//! no prompt at all for a form without fields. The distinction matters for
//! Codex: in Full Access it accepts a form without fields, but declines a form
//! with fields without asking, which reads exactly like the user declining.

use std::time::Duration;

use rmcp::{
    model::{
        ElicitRequest, ElicitRequestParams, ElicitResult, ElicitationAction, ErrorData,
        InputRequest, InputRequests, InputRequiredResult, InputResponses, ProtocolVersion,
        RequestStateCodec, SealOptions,
    },
    service::{ElicitationMode, RequestContext},
    RoleServer,
};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::auth::AuthUser;

const CONFIRM_KEY: &str = "confirm";
/// Client names, from `clientInfo`, of clients that show a form without fields
/// as an approval prompt.
const APPROVAL_PROMPT_CLIENTS: &[&str] = &["codex-mcp-client", "claude-code"];
const CONFIRMATION_TTL: Duration = Duration::from_secs(10 * 60);

pub(super) enum Confirmation {
    /// The user confirmed, or the client cannot ask.
    Proceed,
    /// The user, or the client on their behalf, declined.
    Declined,
    /// The confirmation was dismissed without an answer.
    Cancelled,
    /// Return this to the client, which asks the user and retries.
    Ask(InputRequiredResult),
}

/// The answers a tool call carries from a previous round.
pub(super) struct PriorRound {
    pub request_state: Option<String>,
    pub input_responses: Option<InputResponses>,
}

pub(super) struct Confirmations {
    codec: RequestStateCodec,
}

impl Confirmations {
    /// A codec keyed per process: a restart only expires pending confirmations.
    pub fn new() -> Self {
        let key: [u8; 32] = rand::random();
        Self {
            codec: RequestStateCodec::try_new(key.to_vec())
                .expect("a 32-byte key meets the codec's minimum length"),
        }
    }

    /// Asks the user to confirm `message` before `tool` runs with `arguments`.
    ///
    /// A confirmation is bound to the user, the tool and its exact arguments,
    /// so it cannot approve another call.
    pub async fn confirm(
        &self,
        context: &RequestContext<RoleServer>,
        prior: PriorRound,
        user: &AuthUser,
        tool: &str,
        arguments: &impl serde::Serialize,
        message: String,
    ) -> Result<Confirmation, ErrorData> {
        let binding = binding(user, tool, arguments)?;

        let multi_round = context
            .protocol_version()
            .is_some_and(|version| version >= ProtocolVersion::V_2026_07_28);
        if !multi_round {
            return self.confirm_in_session(context, message).await;
        }

        if let Some(request_state) = prior.request_state {
            self.codec
                .open_with(&request_state, &binding)
                .map_err(|_| {
                    ErrorData::invalid_params(
                        "The confirmation expired or belongs to another call. Call the tool again.",
                        None,
                    )
                })?;
            let answer = prior
                .input_responses
                .and_then(|mut responses| responses.remove(CONFIRM_KEY))
                .ok_or_else(|| {
                    ErrorData::invalid_params("Missing the confirmation answer.", None)
                })?;
            let answer: ElicitResult = serde_json::from_value(answer).map_err(|error| {
                ErrorData::invalid_params(format!("Invalid confirmation answer: {error}"), None)
            })?;
            return Ok(Form::for_client(context).decision(&answer));
        }

        if !client_supports_form_elicitation(context) {
            return Ok(Confirmation::Proceed);
        }

        let request_state = self.codec.seal_with(
            b"confirm",
            &SealOptions::new()
                .associated_data(&binding)
                .ttl(CONFIRMATION_TTL),
        );
        let mut requests = InputRequests::new();
        requests.insert(
            CONFIRM_KEY.to_string(),
            InputRequest::Elicitation(ElicitRequest::new(
                Form::for_client(context).params(message)?,
            )),
        );
        Ok(Confirmation::Ask(InputRequiredResult::new(
            Some(requests),
            Some(request_state),
        )))
    }

    async fn confirm_in_session(
        &self,
        context: &RequestContext<RoleServer>,
        message: String,
    ) -> Result<Confirmation, ErrorData> {
        if !context
            .peer
            .supported_elicitation_modes()
            .contains(&ElicitationMode::Form)
        {
            return Ok(Confirmation::Proceed);
        }

        let form = Form::for_client(context);
        let answer = context
            .peer
            .create_elicitation(form.params(message)?)
            .await
            .map_err(|error| {
                ErrorData::internal_error(format!("Could not ask for confirmation: {error}"), None)
            })?;
        Ok(form.decision(&answer))
    }
}

fn binding(
    user: &AuthUser,
    tool: &str,
    arguments: &impl serde::Serialize,
) -> Result<Vec<u8>, ErrorData> {
    let arguments = serde_json::to_vec(arguments)
        .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;
    let digest = Sha256::digest(arguments);
    let mut binding = format!("{}|{tool}|", user.id).into_bytes();
    binding.extend_from_slice(&digest);
    Ok(binding)
}

fn client_supports_form_elicitation(context: &RequestContext<RoleServer>) -> bool {
    context
        .client_capabilities()
        .and_then(|capabilities| capabilities.elicitation)
        // A capability naming neither mode means form, as it predates modes.
        .is_some_and(|elicitation| elicitation.form.is_some() || elicitation.url.is_none())
}

/// How a client is asked to confirm.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Form {
    /// No fields: accepting confirms.
    Approval,
    /// One required `confirm` checkbox, without a default: VS Code submits
    /// defaults when the user skips the form, which must not confirm.
    Checkbox,
}

impl Form {
    fn for_client(context: &RequestContext<RoleServer>) -> Self {
        Self::for_client_name(context.client_info().map(|info| info.name).as_deref())
    }

    fn for_client_name(name: Option<&str>) -> Self {
        if name.is_some_and(|name| APPROVAL_PROMPT_CLIENTS.contains(&name)) {
            Self::Approval
        } else {
            Self::Checkbox
        }
    }

    fn params(self, message: String) -> Result<ElicitRequestParams, ErrorData> {
        let requested_schema = match self {
            Self::Approval => json!({ "type": "object", "properties": {} }),
            Self::Checkbox => json!({
                "type": "object",
                "properties": {
                    CONFIRM_KEY: {
                        "type": "boolean",
                        "title": "Confirm",
                        "description": "Run this change in Toki."
                    }
                },
                "required": [CONFIRM_KEY]
            }),
        };
        let requested_schema = serde_json::from_value(requested_schema)
            .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;

        Ok(ElicitRequestParams::FormElicitationParams {
            meta: None,
            message,
            requested_schema,
        })
    }

    fn decision(self, answer: &ElicitResult) -> Confirmation {
        let checked = || {
            answer
                .content
                .as_ref()
                .and_then(|content| content.get(CONFIRM_KEY))
                .and_then(serde_json::Value::as_bool)
                == Some(true)
        };
        match answer.action {
            ElicitationAction::Accept if self == Self::Approval || checked() => {
                Confirmation::Proceed
            }
            ElicitationAction::Accept | ElicitationAction::Decline => Confirmation::Declined,
            _ => Confirmation::Cancelled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{models::UserId, AuthMethod, Role, UserPrincipal};

    fn user(id: i32) -> AuthUser {
        AuthUser::new(
            UserPrincipal {
                id: UserId::new(id),
                email: "ada@example.com".to_string(),
                roles: vec![Role::User],
            },
            AuthMethod::ApiToken,
        )
    }

    #[test]
    fn a_confirmation_binds_the_user_tool_and_arguments() {
        let confirmations = Confirmations::new();
        let arguments = json!({ "projectRegistrationId": "42" });
        let bound = binding(&user(7), "delete_time_entry", &arguments).unwrap();
        let sealed = confirmations
            .codec
            .seal_with(b"confirm", &SealOptions::new().associated_data(&bound));

        assert!(confirmations.codec.open_with(&sealed, &bound).is_ok());
        for other in [
            binding(&user(8), "delete_time_entry", &arguments).unwrap(),
            binding(&user(7), "delete_ai_subscription", &arguments).unwrap(),
            binding(
                &user(7),
                "delete_time_entry",
                &json!({ "projectRegistrationId": "43" }),
            )
            .unwrap(),
        ] {
            assert!(confirmations.codec.open_with(&sealed, &other).is_err());
        }
    }

    #[test]
    fn known_approval_prompt_clients_get_a_form_without_fields() {
        assert_eq!(
            Form::for_client_name(Some("codex-mcp-client")),
            Form::Approval
        );
        assert_eq!(Form::for_client_name(Some("claude-code")), Form::Approval);
        assert_eq!(
            Form::for_client_name(Some("Visual Studio Code")),
            Form::Checkbox
        );
        assert_eq!(Form::for_client_name(None), Form::Checkbox);
    }

    #[test]
    fn only_an_accepted_confirmation_proceeds() {
        let answer =
            |answer: serde_json::Value| -> ElicitResult { serde_json::from_value(answer).unwrap() };
        let proceeds = |form: Form, answer: &ElicitResult| {
            matches!(form.decision(answer), Confirmation::Proceed)
        };

        let accepted = answer(json!({ "action": "accept" }));
        assert!(proceeds(Form::Approval, &accepted));
        assert!(!proceeds(Form::Checkbox, &accepted));

        let checked = answer(json!({ "action": "accept", "content": { CONFIRM_KEY: true } }));
        assert!(proceeds(Form::Checkbox, &checked));
        let unchecked = answer(json!({ "action": "accept", "content": { CONFIRM_KEY: false } }));
        assert!(matches!(
            Form::Checkbox.decision(&unchecked),
            Confirmation::Declined
        ));

        for form in [Form::Approval, Form::Checkbox] {
            assert!(matches!(
                form.decision(&answer(json!({ "action": "decline" }))),
                Confirmation::Declined
            ));
            assert!(matches!(
                form.decision(&answer(json!({ "action": "cancel" }))),
                Confirmation::Cancelled
            ));
        }
    }
}

//! Human confirmation for destructive tools, through MCP elicitation.
//!
//! Clients on protocol `2026-07-28` and later get a multi round-trip request:
//! the tool returns an elicitation and a sealed `requestState`, and the client
//! retries with the user's answer. Older clients get a server-to-client
//! elicitation request within their session. A client that supports neither
//! runs the tool directly: its own approval of destructive tools, which tool
//! annotations announce, is then the only confirmation.

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
const CONFIRMATION_TTL: Duration = Duration::from_secs(10 * 60);

pub(super) enum Confirmation {
    /// The user confirmed, or the client cannot ask.
    Proceed,
    /// The user declined or cancelled.
    Declined,
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
            return Ok(decision(&answer));
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
            InputRequest::Elicitation(ElicitRequest::new(confirmation_form(message)?)),
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

        let answer = context
            .peer
            .create_elicitation(confirmation_form(message)?)
            .await
            .map_err(|error| {
                ErrorData::internal_error(format!("Could not ask for confirmation: {error}"), None)
            })?;
        Ok(decision(&answer))
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

fn confirmation_form(message: String) -> Result<ElicitRequestParams, ErrorData> {
    let requested_schema = serde_json::from_value(json!({
        "type": "object",
        "properties": {
            CONFIRM_KEY: {
                "type": "boolean",
                "title": "Confirm",
                "description": "Run this change in Toki.",
                "default": true
            }
        },
        "required": [CONFIRM_KEY]
    }))
    .map_err(|error| ErrorData::internal_error(error.to_string(), None))?;

    Ok(ElicitRequestParams::FormElicitationParams {
        meta: None,
        message,
        requested_schema,
    })
}

fn decision(answer: &ElicitResult) -> Confirmation {
    let confirmed = answer
        .content
        .as_ref()
        .and_then(|content| content.get(CONFIRM_KEY))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);

    if answer.action == ElicitationAction::Accept && confirmed {
        Confirmation::Proceed
    } else {
        Confirmation::Declined
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
    fn only_an_accepted_confirmation_proceeds() {
        let answer = |action: &str, confirm: bool| -> ElicitResult {
            serde_json::from_value(json!({
                "action": action,
                "content": { CONFIRM_KEY: confirm },
            }))
            .unwrap()
        };

        assert!(matches!(
            decision(&answer("accept", true)),
            Confirmation::Proceed
        ));
        assert!(matches!(
            decision(&answer("accept", false)),
            Confirmation::Declined
        ));
        assert!(matches!(
            decision(&answer("decline", true)),
            Confirmation::Declined
        ));
        assert!(matches!(
            decision(&answer("cancel", true)),
            Confirmation::Declined
        ));
    }
}

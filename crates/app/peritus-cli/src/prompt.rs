use std::{ffi::OsStr, path::Path, time::Duration};

use peritus_app_protocol::{
    AppRequestPayload, AppResponsePayload, PromptAnswer, PromptAnswerPayload, PromptCancellation,
    PromptKind, SignedApprovalDecisionFrame, UserInputValue, WellKnownProtocolFeature,
    decode_prompt_binding_value,
};
use peritus_types::SessionId;

use crate::{
    args::{PromptAnswerArgs, PromptCancelArgs, PromptValue},
    client::Client,
    error::CliError,
    id::hex,
    operation::response_error,
    output::Output,
};

pub async fn answer(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Duration,
    arguments: PromptAnswerArgs,
    output: &Output,
) -> Result<(), CliError> {
    let required = match &arguments.value {
        PromptValue::SignedDecision(_) => WellKnownProtocolFeature::ApprovalPrompts,
        _ => WellKnownProtocolFeature::UserInput,
    };
    let mut client = Client::connect(endpoint, session, timeout, &[required]).await?;
    let binding_bytes = read(&arguments.binding, "read prompt binding").await?;
    let binding = decode_prompt_binding_value(&binding_bytes, client.limits())?;
    ensure_prompt_session(&client, &binding)?;
    let maximum = client.limits().codec().max_string_bytes;
    let payload = match arguments.value {
        PromptValue::SignedDecision(path) => {
            if binding.kind() != PromptKind::Approval {
                return Err(CliError::usage(
                    "--signed-decision can answer only an approval prompt binding",
                ));
            }
            let bytes = read(&path, "read signed approval decision").await?;
            let decision =
                SignedApprovalDecisionFrame::new(bytes, client.limits().codec().max_frame_bytes)
                    .map_err(|error| CliError::usage(error.to_string()))?;
            PromptAnswerPayload::signed_approval(decision, arguments.rationale, maximum)
                .map_err(|error| CliError::usage(error.to_string()))?
        }
        PromptValue::Text(value) => {
            ensure_user_input(binding.kind(), arguments.rationale.as_ref())?;
            PromptAnswerPayload::UserInput(
                UserInputValue::text(value, maximum)
                    .map_err(|error| CliError::usage(error.to_string()))?,
            )
        }
        PromptValue::Selection(value) => {
            ensure_user_input(binding.kind(), arguments.rationale.as_ref())?;
            PromptAnswerPayload::UserInput(
                UserInputValue::selection(value, maximum)
                    .map_err(|error| CliError::usage(error.to_string()))?,
            )
        }
        PromptValue::Confirmation(value) => {
            ensure_user_input(binding.kind(), arguments.rationale.as_ref())?;
            PromptAnswerPayload::UserInput(UserInputValue::confirmation(value))
        }
        PromptValue::SecretReference(value) => {
            ensure_user_input(binding.kind(), arguments.rationale.as_ref())?;
            PromptAnswerPayload::UserInput(
                UserInputValue::secret_reference(value, maximum)
                    .map_err(|error| CliError::usage(error.to_string()))?,
            )
        }
    };
    let answer = PromptAnswer::new(binding.correlation(), payload, maximum)
        .map_err(|error| CliError::usage(error.to_string()))?;
    let identity = Client::new_request_identity()?;
    let response = client.request(identity, AppRequestPayload::AnswerPrompt(answer)).await?;
    let AppResponsePayload::PromptAccepted(prompt_id) = response.payload() else {
        return response_error(response.payload(), "prompt acceptance");
    };
    if *prompt_id != binding.correlation().prompt_id() {
        return Err(CliError::protocol(
            "validate prompt acceptance",
            "daemon accepted a different prompt identity",
        ));
    }
    output.success(
        "prompt-answered",
        serde_json::json!({
            "prompt_id": hex(prompt_id.as_bytes()),
            "accepted": true,
            "authority_granted": false,
        }),
        &format!(
            "prompt {} answer accepted as protocol input; authority is decided separately",
            hex(prompt_id.as_bytes()),
        ),
    )
}

pub async fn cancel(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Duration,
    arguments: PromptCancelArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mut client = Client::connect(
        endpoint,
        session,
        timeout,
        &[WellKnownProtocolFeature::ApprovalPrompts, WellKnownProtocolFeature::UserInput],
    )
    .await?;
    let bytes = read(&arguments.binding, "read prompt binding").await?;
    let binding = decode_prompt_binding_value(&bytes, client.limits())?;
    ensure_prompt_session(&client, &binding)?;
    let identity = Client::new_request_identity()?;
    let cancellation = PromptCancellation::new(binding.correlation(), identity.correlation_id);
    let response = client.request(identity, AppRequestPayload::CancelPrompt(cancellation)).await?;
    let AppResponsePayload::PromptAccepted(prompt_id) = response.payload() else {
        return response_error(response.payload(), "prompt cancellation acceptance");
    };
    if *prompt_id != binding.correlation().prompt_id() {
        return Err(CliError::protocol(
            "validate prompt cancellation",
            "daemon accepted cancellation for a different prompt",
        ));
    }
    output.success(
        "prompt-cancelled",
        serde_json::json!({ "prompt_id": hex(prompt_id.as_bytes()), "accepted": true }),
        &format!("prompt {} cancellation accepted", hex(prompt_id.as_bytes())),
    )
}

fn ensure_prompt_session(
    client: &Client,
    binding: &peritus_app_protocol::PromptBinding,
) -> Result<(), CliError> {
    if binding.correlation().session_id() == client.context().session_id() {
        Ok(())
    } else {
        Err(CliError::usage(
            "prompt binding belongs to another durable session; pass its --session",
        ))
    }
}

fn ensure_user_input(kind: PromptKind, rationale: Option<&String>) -> Result<(), CliError> {
    if kind != PromptKind::UserInput {
        return Err(CliError::usage(
            "user-input answer can answer only a user-input prompt binding",
        ));
    }
    if rationale.is_some() {
        return Err(CliError::usage("--rationale is valid only with --signed-decision"));
    }
    Ok(())
}

async fn read(path: &Path, operation: &'static str) -> Result<Vec<u8>, CliError> {
    tokio::fs::read(path)
        .await
        .map_err(|error| CliError::local_io(operation, Some(path.to_path_buf()), error))
}

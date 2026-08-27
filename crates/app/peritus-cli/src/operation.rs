use std::{ffi::OsStr, path::Path, time::Duration};

use peritus_app_protocol::{
    AppEventPayload, AppRequestPayload, AppResponsePayload, CommandBinding, CommandDisposition,
    CommandSubmissionFrames, DaemonReadiness, IdempotencyKey, ShutdownCompletionDisposition,
    ShutdownRequest, WellKnownProtocolFeature,
};
use peritus_types::{ActorId, SessionId};

use crate::{args::SubmitArgs, client::Client, error::CliError, id::hex, output::Output};

pub async fn status(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Duration,
    output: &Output,
) -> Result<(), CliError> {
    let mut client = Client::connect(
        endpoint,
        session,
        timeout,
        &[WellKnownProtocolFeature::ReadOnlyDiagnostics],
    )
    .await?;
    let identity = Client::new_request_identity()?;
    let response = client.request(identity, AppRequestPayload::DaemonStatus).await?;
    let AppResponsePayload::DaemonStatus(status) = response.payload() else {
        return response_error(response.payload(), "daemon status");
    };
    let readiness = readiness_name(status.readiness());
    let session_id = hex(client.context().session_id().as_bytes());
    output.success(
        "daemon-status",
        serde_json::json!({
            "readiness": readiness,
            "mutation_ready": status.mutation_ready(),
            "diagnostic": status.diagnostic(),
            "session_id": session_id,
            "protocol": {
                "major": client.context().version().major(),
                "minor": client.context().version().minor(),
            }
        }),
        &format!(
            "daemon {readiness}; session={session_id}{}",
            status.diagnostic().map_or_else(String::new, |text| format!("; {text}")),
        ),
    )
}

pub async fn shutdown(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Duration,
    wait: bool,
    output: &Output,
) -> Result<(), CliError> {
    let mut client =
        Client::connect(endpoint, session, timeout, &[WellKnownProtocolFeature::GracefulShutdown])
            .await?;
    let identity = Client::new_request_identity()?;
    let shutdown = ShutdownRequest::new(identity.request_id, identity.correlation_id);
    let response = client.request(identity, AppRequestPayload::Shutdown(shutdown)).await?;
    let AppResponsePayload::ShutdownAccepted(accepted) = response.payload() else {
        return response_error(response.payload(), "shutdown acceptance");
    };
    if accepted.request() != shutdown {
        return Err(CliError::protocol(
            "validate shutdown acceptance",
            "daemon accepted a different shutdown request",
        ));
    }
    if !wait {
        return output.success(
            "shutdown-accepted",
            serde_json::json!({
                "request_id": hex(shutdown.request_id().as_bytes()),
                "correlation_id": hex(shutdown.correlation_id().as_bytes()),
                "completed": false,
            }),
            "shutdown accepted; completion not awaited",
        );
    }

    loop {
        let event = client.read_event().await?;
        if client.reply_heartbeat(&event).await? {
            continue;
        }
        match event.payload() {
            AppEventPayload::ShutdownProgress(progress) if progress.request() == shutdown => {
                let remaining = progress
                    .remaining()
                    .iter()
                    .map(|item| {
                        serde_json::json!({
                            "kind": remaining_kind(item.kind()),
                            "descriptor": item.descriptor(),
                        })
                    })
                    .collect::<Vec<_>>();
                output.event(
                    serde_json::json!({
                        "ok": true,
                        "kind": "shutdown-progress",
                        "completed_steps": progress.completed_steps(),
                        "total_steps": progress.total_steps(),
                        "remaining": remaining,
                    }),
                    &format!(
                        "shutdown progress {}/{}; remaining={}",
                        progress.completed_steps(),
                        progress.total_steps(),
                        progress.remaining().len(),
                    ),
                )?;
            }
            AppEventPayload::ShutdownComplete(complete) if complete.request() == shutdown => {
                let disposition = match complete.disposition() {
                    ShutdownCompletionDisposition::Clean => "clean",
                    ShutdownCompletionDisposition::Unclean => "unclean",
                };
                output.success(
                    "shutdown-complete",
                    serde_json::json!({
                        "disposition": disposition,
                        "remaining": complete.remaining().iter().map(|item| serde_json::json!({
                            "kind": remaining_kind(item.kind()),
                            "descriptor": item.descriptor(),
                        })).collect::<Vec<_>>(),
                    }),
                    &format!("shutdown {disposition}; remaining={}", complete.remaining().len()),
                )?;
                if complete.disposition() == ShutdownCompletionDisposition::Clean {
                    return Ok(());
                }
                return Err(CliError::protocol(
                    "wait for shutdown",
                    "daemon reported unclean shutdown completion",
                ));
            }
            AppEventPayload::Diagnostic(diagnostic) => {
                output.event(
                    serde_json::json!({ "ok": true, "kind": "diagnostic", "message": diagnostic.as_str() }),
                    diagnostic.as_str(),
                )?;
            }
            _ => {}
        }
    }
}

pub async fn submit(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Duration,
    arguments: SubmitArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mut client = Client::connect(endpoint, session, timeout, &[]).await?;
    let envelope = read(&arguments.envelope, "read command envelope").await?;
    let payload = read(&arguments.payload, "read command payload").await?;
    let frames = CommandSubmissionFrames::parse(envelope, payload, client.limits())?;
    let expected_revision =
        arguments.bind_expected_revision.then(|| frames.envelope().as_domain().revision());
    let identity = Client::new_request_identity()?;
    let actor =
        ActorId::new(arguments.actor).map_err(|_| CliError::usage("invalid --actor identifier"))?;
    let key = IdempotencyKey::new(arguments.idempotency_key)
        .map_err(|error| CliError::usage(format!("invalid --idempotency-key: {error}")))?;
    let binding = CommandBinding::new(
        actor,
        client.context().session_id(),
        identity.request_id,
        identity.correlation_id,
        key,
        expected_revision,
        frames,
    )?;
    let digest = hex(binding.request_digest().as_bytes());
    let response = client.request(identity, AppRequestPayload::SubmitCommand(binding)).await?;
    let AppResponsePayload::CommandResult(result) = response.payload() else {
        return response_error(response.payload(), "command result");
    };
    if let Some(error) = result.error() {
        return Err(CliError::rejected(error));
    }
    let disposition = match result.disposition() {
        CommandDisposition::Committed => "committed",
        CommandDisposition::Replayed => "replayed",
        CommandDisposition::Rejected => {
            return Err(CliError::protocol(
                "validate command result",
                "rejected result omitted its application error",
            ));
        }
    };
    let range = result.committed_events().ok_or_else(|| {
        CliError::protocol("validate command result", "successful result omitted committed range")
    })?;
    output.success(
        "command-result",
        serde_json::json!({
            "disposition": disposition,
            "request_id": hex(result.original_request_id().as_bytes()),
            "request_digest": digest,
            "committed_events": {
                "first": range.first().get(),
                "last": range.last().get(),
                "count": range.count(),
            },
            "session_id": hex(client.context().session_id().as_bytes()),
        }),
        &format!(
            "command {disposition}; events={}..{} ({}); request-digest={digest}",
            range.first().get(),
            range.last().get(),
            range.count(),
        ),
    )
}

pub fn response_error(
    payload: &AppResponsePayload,
    expected: &'static str,
) -> Result<(), CliError> {
    match payload {
        AppResponsePayload::Error(error) => Err(CliError::rejected(error)),
        _ => Err(CliError::protocol(
            "validate daemon response",
            format!("expected {expected}, received another response payload"),
        )),
    }
}

async fn read(path: &Path, operation: &'static str) -> Result<Vec<u8>, CliError> {
    tokio::fs::read(path)
        .await
        .map_err(|error| CliError::local_io(operation, Some(path.to_path_buf()), error))
}

const fn readiness_name(readiness: DaemonReadiness) -> &'static str {
    match readiness {
        DaemonReadiness::Starting => "starting",
        DaemonReadiness::ReadyReadWrite => "ready-read-write",
        DaemonReadiness::ReadyReadOnly => "ready-read-only",
        DaemonReadiness::Draining => "draining",
        DaemonReadiness::Unavailable => "unavailable",
    }
}

const fn remaining_kind(kind: peritus_app_protocol::RemainingWorkKind) -> &'static str {
    match kind {
        peritus_app_protocol::RemainingWorkKind::Request => "request",
        peritus_app_protocol::RemainingWorkKind::Subscription => "subscription",
        peritus_app_protocol::RemainingWorkKind::ArtifactTransfer => "artifact-transfer",
        peritus_app_protocol::RemainingWorkKind::TerminalAttachment => "terminal-attachment",
        peritus_app_protocol::RemainingWorkKind::Other => "other",
    }
}

use std::{ffi::OsStr, path::Path, time::Duration};

use peritus_app_protocol::{
    AppEventPayload, AppRequestPayload, AppResponsePayload, CommandBinding, CommandDisposition,
    CommandSubmissionFrames, DaemonReadiness, IdempotencyKey, ShutdownCompletionDisposition,
    CorrelationId, RequestId, ShutdownRequest, WellKnownProtocolFeature,
};
use peritus_types::{ActorId, SessionId};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio::io::AsyncReadExt as _;

use crate::{
    args::SubmitArgs,
    client::{Client, RequestIdentity},
    error::CliError,
    id::{hex, parse_hex_id},
    output::Output,
    recovery,
};

pub async fn status(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Option<Duration>,
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
    timeout: Option<Duration>,
    wait: bool,
    output: &Output,
) -> Result<(), CliError> {
    let completion_deadline = if wait { checked_deadline(timeout)? } else { None };
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
    await_shutdown(&mut client, shutdown, completion_deadline, output).await
}

async fn await_shutdown(
    client: &mut Client,
    shutdown: ShutdownRequest,
    deadline: Option<tokio::time::Instant>,
    output: &Output,
) -> Result<(), CliError> {
    loop {
        let remaining = deadline
            .map(|deadline| deadline.saturating_duration_since(tokio::time::Instant::now()));
        if remaining.is_some_and(|remaining| remaining.is_zero()) {
            return Err(CliError::connection(
                "wait for shutdown",
                "shutdown completion deadline elapsed after acceptance",
            ));
        }
        let event = crate::client::optional_timeout(remaining, client.read_event())
            .await
            .map_err(|_| {
                CliError::connection(
                    "wait for shutdown",
                    "shutdown completion deadline elapsed after acceptance",
                )
            })??;
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

fn checked_deadline(
    timeout: Option<Duration>,
) -> Result<Option<tokio::time::Instant>, CliError> {
    timeout
        .map(|duration| {
            tokio::time::Instant::now().checked_add(duration).ok_or_else(|| {
                CliError::usage("--timeout-seconds is too large for this platform")
            })
        })
        .transpose()
}

pub async fn submit(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Option<Duration>,
    arguments: SubmitArgs,
    output: &Output,
) -> Result<(), CliError> {
    let _receipt_lease = recovery::lease(arguments.receipt.as_deref()).await?;
    let retained = match arguments.receipt.as_deref() {
        Some(path) => recovery::load::<CommandReceipt>(path).await?,
        None => None,
    };
    if let Some(receipt) = &retained {
        validate_command_receipt_header(receipt)?;
    }
    let retained_session = retained
        .as_ref()
        .map(|receipt| stored_session(&receipt.session_id))
        .transpose()?;
    if session.is_some() && retained_session.is_some() && session != retained_session {
        return Err(CliError::usage(
            "--session does not match the durable session in --receipt",
        ));
    }
    let requested_session = retained_session.or(session);
    let mut client = Client::connect(endpoint, requested_session, timeout, &[]).await?;
    let maximum = client.limits().codec().max_frame_bytes;
    let envelope = read_bounded(&arguments.envelope, maximum, "read command envelope").await?;
    let payload = read_bounded(&arguments.payload, maximum, "read command payload").await?;
    let frames = CommandSubmissionFrames::parse(envelope, payload, client.limits())?;
    let expected_revision =
        arguments.bind_expected_revision.then(|| frames.envelope().as_domain().revision());
    let actor =
        ActorId::new(arguments.actor).map_err(|_| CliError::usage("invalid --actor identifier"))?;
    let key = IdempotencyKey::new(arguments.idempotency_key)
        .map_err(|error| CliError::usage(format!("invalid --idempotency-key: {error}")))?;
    let identity = match retained.as_ref() {
        Some(receipt) => restored_request_identity(receipt)?,
        None => Client::new_request_identity()?,
    };
    let envelope_digest = hex(frames.envelope_frame().digest().as_bytes());
    let payload_digest = hex(frames.command_frame().digest().as_bytes());
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
    let scope = command_scope_fingerprint(
        endpoint,
        client.context().session_id(),
        actor,
        key.as_bytes(),
        arguments.bind_expected_revision,
        &envelope_digest,
        &payload_digest,
    );
    let was_new = retained.is_none();
    let mut receipt = match retained {
        Some(receipt) => {
            if receipt.scope_sha256 != scope
                || receipt.request_digest != digest
                || receipt.envelope_sha256 != envelope_digest
                || receipt.payload_sha256 != payload_digest
            {
                return Err(CliError::usage(
                    "--receipt belongs to different command bytes, scope, or policy",
                ));
            }
            receipt
        }
        None => CommandReceipt {
            version: 1,
            kind: "command-submit".to_owned(),
            session_id: hex(client.context().session_id().as_bytes()),
            request_id: hex(identity.request_id.as_bytes()),
            correlation_id: hex(identity.correlation_id.as_bytes()),
            scope_sha256: scope,
            envelope_sha256: envelope_digest,
            payload_sha256: payload_digest,
            request_digest: digest.clone(),
            phase: CommandReceiptPhase::Prepared,
            disposition: None,
            first_event: None,
            last_event: None,
            event_count: None,
            remote_code_tag: None,
            output_delivered: false,
        },
    };
    if let Some(path) = arguments.receipt.as_deref() {
        if was_new {
            recovery::create(path, &receipt).await?;
        }
    }
    if receipt.phase == CommandReceiptPhase::Settled {
        let prior_output = if receipt.output_delivered {
            "confirmed"
        } else {
            "possible"
        };
        return render_retained_command(
            &mut receipt,
            arguments.receipt.as_deref(),
            output,
            Some(prior_output),
        )
        .await;
    }
    if receipt.phase == CommandReceiptPhase::Rejected {
        let tag = receipt.remote_code_tag.ok_or_else(|| {
            CliError::runtime("validate operation receipt", "rejected command omitted its code")
        })?;
        let code = peritus_app_protocol::AppErrorCode::from_tag(tag).ok_or_else(|| {
            CliError::runtime("validate operation receipt", "receipt contains an unknown error")
        })?;
        return Err(CliError::rejected(&peritus_app_protocol::AppProtocolError::new(
            code, None,
        )));
    }
    let response = client.request(identity, AppRequestPayload::SubmitCommand(binding)).await?;
    let AppResponsePayload::CommandResult(result) = response.payload() else {
        return response_error(response.payload(), "command result");
    };
    if result.original_request_id() != identity.request_id {
        return Err(CliError::protocol(
            "validate command result",
            "daemon returned a result for another original request",
        ));
    }
    if let Some(error) = result.error() {
        receipt.phase = CommandReceiptPhase::Rejected;
        receipt.remote_code_tag = Some(error.code().tag());
        persist_command_receipt(arguments.receipt.as_deref(), &receipt).await?;
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
    receipt.phase = CommandReceiptPhase::Settled;
    receipt.disposition = Some(disposition.to_owned());
    receipt.first_event = Some(range.first().get());
    receipt.last_event = Some(range.last().get());
    receipt.event_count = Some(range.count());
    receipt.output_delivered = false;
    persist_command_receipt(arguments.receipt.as_deref(), &receipt).await?;
    render_retained_command(
        &mut receipt,
        arguments.receipt.as_deref(),
        output,
        None,
    )
    .await
}

async fn render_retained_command(
    receipt: &mut CommandReceipt,
    path: Option<&Path>,
    output: &Output,
    prior_output: Option<&str>,
) -> Result<(), CliError> {
    let disposition = receipt.disposition.as_deref().ok_or_else(|| {
        CliError::runtime("validate operation receipt", "settled command omitted disposition")
    })?;
    let first = receipt.first_event.ok_or_else(|| {
        CliError::runtime("validate operation receipt", "settled command omitted first event")
    })?;
    let last = receipt.last_event.ok_or_else(|| {
        CliError::runtime("validate operation receipt", "settled command omitted last event")
    })?;
    let count = receipt.event_count.ok_or_else(|| {
        CliError::runtime("validate operation receipt", "settled command omitted event count")
    })?;
    output.success(
        "command-result",
        serde_json::json!({
            "disposition": disposition,
            "request_id": receipt.request_id.as_str(),
            "request_digest": receipt.request_digest.as_str(),
            "committed_events": {
                "first": first,
                "last": last,
                "count": count,
            },
            "session_id": receipt.session_id.as_str(),
            "receipt": path.map(|path| path.display().to_string()),
            "prior_output": prior_output,
        }),
        &format!(
            "command {disposition}; events={first}..{last} ({count}); request-digest={}{}",
            receipt.request_digest,
            prior_output
                .map(|state| format!("; prior output {state}"))
                .unwrap_or_default(),
        ),
    )?;
    receipt.output_delivered = true;
    persist_command_receipt(path, receipt).await
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

async fn read_bounded(
    path: &Path,
    maximum: usize,
    operation: &'static str,
) -> Result<Vec<u8>, CliError> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|error| CliError::local_io(operation, Some(path.to_path_buf()), error))?;
    let metadata = file
        .metadata()
        .await
        .map_err(|error| CliError::local_io(operation, Some(path.to_path_buf()), error))?;
    if metadata.len() > maximum as u64 {
        return Err(CliError::usage(format!(
            "{} exceeds the negotiated {maximum}-byte frame limit",
            path.display(),
        )));
    }
    let mut bytes = Vec::with_capacity(
        usize::try_from(metadata.len()).unwrap_or(maximum).min(maximum),
    );
    file.take((maximum as u64).saturating_add(1))
        .read_to_end(&mut bytes)
        .await
        .map_err(|error| CliError::local_io(operation, Some(path.to_path_buf()), error))?;
    if bytes.len() > maximum {
        return Err(CliError::usage(format!(
            "{} exceeds the negotiated {maximum}-byte frame limit",
            path.display(),
        )));
    }
    Ok(bytes)
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum CommandReceiptPhase {
    Prepared,
    Settled,
    Rejected,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CommandReceipt {
    version: u32,
    kind: String,
    session_id: String,
    request_id: String,
    correlation_id: String,
    scope_sha256: String,
    envelope_sha256: String,
    payload_sha256: String,
    request_digest: String,
    phase: CommandReceiptPhase,
    disposition: Option<String>,
    first_event: Option<u64>,
    last_event: Option<u64>,
    event_count: Option<u64>,
    remote_code_tag: Option<u16>,
    output_delivered: bool,
}

fn validate_command_receipt_header(receipt: &CommandReceipt) -> Result<(), CliError> {
    if receipt.version != 1 || receipt.kind != "command-submit" {
        return Err(CliError::runtime(
            "validate operation receipt",
            "receipt is not a supported command-submit receipt",
        ));
    }
    Ok(())
}

fn stored_session(value: &str) -> Result<SessionId, CliError> {
    SessionId::new(stored_id(value, "session")?)
        .map_err(|_| CliError::runtime("validate operation receipt", "invalid session identity"))
}

fn restored_request_identity(receipt: &CommandReceipt) -> Result<RequestIdentity, CliError> {
    let request = RequestId::new(stored_id(&receipt.request_id, "request")?)
        .map_err(|_| CliError::runtime("validate operation receipt", "invalid request identity"))?;
    let correlation = CorrelationId::new(stored_id(&receipt.correlation_id, "correlation")?)
        .map_err(|_| {
            CliError::runtime("validate operation receipt", "invalid correlation identity")
        })?;
    Ok(RequestIdentity::new(request, correlation))
}

fn stored_id(value: &str, field: &str) -> Result<[u8; 16], CliError> {
    parse_hex_id(value, field).map_err(|_| {
        CliError::runtime(
            "validate operation receipt",
            format!("receipt contains an invalid {field} identity"),
        )
    })
}

fn command_scope_fingerprint(
    endpoint: &OsStr,
    session: SessionId,
    actor: ActorId,
    key: &[u8],
    bind_expected_revision: bool,
    envelope_digest: &str,
    payload_digest: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/cli-command-scope/v1\0");
    fingerprint_part(&mut hasher, endpoint.as_encoded_bytes());
    fingerprint_part(&mut hasher, session.as_bytes());
    fingerprint_part(&mut hasher, actor.as_bytes());
    fingerprint_part(&mut hasher, key);
    hasher.update([u8::from(bind_expected_revision)]);
    fingerprint_part(&mut hasher, envelope_digest.as_bytes());
    fingerprint_part(&mut hasher, payload_digest.as_bytes());
    let digest: [u8; 32] = hasher.finalize().into();
    hex(&digest)
}

fn fingerprint_part(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

async fn persist_command_receipt(
    path: Option<&Path>,
    receipt: &CommandReceipt,
) -> Result<(), CliError> {
    match path {
        Some(path) => recovery::replace(path, receipt).await,
        None => Ok(()),
    }
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

use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use peritus_app_protocol::{
    AppErrorCode, AppEventPayload, AppRequestPayload, AppResponsePayload, CorrelationId,
    RequestId, TerminalAttachmentId, TerminalBinding, TerminalCancellation, TerminalDetach,
    TerminalExit, TerminalExitDisposition, TerminalInput, TerminalResize, TerminalState,
    TerminalStream, WellKnownProtocolFeature,
};
use peritus_types::{ProcessId, SessionId};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use tokio::io::{AsyncRead, AsyncReadExt as _};

use crate::{
    args::{
        TerminalAttachArgs, TerminalBindingArgs, TerminalInputArgs, TerminalInterruptAction,
        TerminalResizeArgs,
    },
    client::{Client, RequestIdentity},
    error::{CliError, ExitCategory},
    id::{generated_id, hex, parse_hex_id},
    operation::response_error,
    output::{Output, TerminalSanitizer, TerminalSanitizerSnapshot},
    recovery,
};

const RECONNECT_DELAY: Duration = Duration::from_millis(250);
const PORTABLE_NATIVE_TERMINAL_DIMENSION_MAX: u16 = i16::MAX as u16;

pub async fn attach(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Option<Duration>,
    arguments: TerminalAttachArgs,
    output: &Output,
) -> Result<(), CliError> {
    let _receipt_lease = recovery::lease(arguments.receipt.as_deref()).await?;
    let scope = follow_scope_fingerprint(endpoint, &arguments);
    let retained = match arguments.receipt.as_deref() {
        Some(path) => recovery::load::<FollowReceipt>(path).await?,
        None => None,
    };
    if let Some(receipt) = &retained {
        validate_follow_receipt(receipt, &scope, session)?;
        if receipt.phase.is_terminal() {
            return render_follow_receipt(receipt, arguments.receipt.as_deref(), output);
        }
    }

    let requested_session = retained
        .as_ref()
        .map(|receipt| stored_session(&receipt.session_id))
        .transpose()?
        .or(session);
    let mut client = connect(endpoint, requested_session, timeout).await?;
    let mut receipt = match retained {
        Some(receipt) => receipt,
        None => {
            let process = ProcessId::new(arguments.process)
                .map_err(|_| CliError::usage("invalid --process identifier"))?;
            let identity = Client::new_request_identity()?;
            let attachment = TerminalAttachmentId::new(generated_id(b"terminal"))
                .map_err(|_| {
                    CliError::runtime(
                        "create terminal attachment",
                        "generated zero identifier",
                    )
                })?;
            let receipt = FollowReceipt {
                version: 1,
                kind: "terminal-follow".to_owned(),
                scope_sha256: scope,
                session_id: hex(client.context().session_id().as_bytes()),
                attachment_id: hex(attachment.as_bytes()),
                process_id: hex(process.as_bytes()),
                originating_request_id: hex(identity.request_id.as_bytes()),
                attach_correlation_id: hex(identity.correlation_id.as_bytes()),
                next_sequence: 0,
                next_offset: 0,
                stream_offsets: [0; 3],
                sanitizers: std::array::from_fn(|_| TerminalSanitizer::default().snapshot()),
                mode: None,
                phase: FollowPhase::Prepared,
                control_request_id: None,
                control_correlation_id: None,
                exit_kind: None,
                exit_value: None,
                exit_successful: None,
                unavailable_code: None,
            };
            if let Some(path) = arguments.receipt.as_deref() {
                recovery::create(path, &receipt).await?;
            }
            receipt
        }
    };
    if receipt.session_id != hex(client.context().session_id().as_bytes()) {
        return Err(CliError::protocol(
            "resume terminal attachment",
            "daemon established a different durable session",
        ));
    }
    let binding = receipt_binding(&receipt)?;
    let mut sanitizers = restored_sanitizers(&receipt)?;

    if receipt.phase.is_control_prepared() {
        complete_interrupt(
            endpoint,
            timeout,
            &arguments,
            binding,
            &mut receipt,
            output,
        )
        .await?;
        return render_follow_receipt(&receipt, arguments.receipt.as_deref(), output);
    }

    loop {
        match establish_attachment(&mut client, binding, &mut receipt, &arguments, true).await {
            Ok(()) => {}
            Err(error) if reconnectable(&error) => {
                drop(client);
                client = reconnect_for_follow(
                    endpoint,
                    timeout,
                    &arguments,
                    binding,
                    &mut receipt,
                    output,
                )
                .await?;
                continue;
            }
            Err(error) if retryable_validation(&error) => {
                drop(client);
                client = reconnect_for_follow(
                    endpoint,
                    timeout,
                    &arguments,
                    binding,
                    &mut receipt,
                    output,
                )
                .await?;
                continue;
            }
            Err(error)
                if error
                    .remote()
                    .is_some_and(|remote| remote.code == AppErrorCode::TerminalState) =>
            {
                receipt.phase = FollowPhase::Unavailable;
                receipt.unavailable_code =
                    error.remote().map(|remote| u64::from(remote.code.tag()));
                persist(arguments.receipt.as_deref(), &receipt).await?;
                return Err(error);
            }
            Err(error) => return Err(error),
        }

        announce_attachment(&receipt, binding, &arguments, output)?;
        if !arguments.follow {
            return Ok(());
        }

        let mut state =
            TerminalState::new(binding, client.limits().max_terminal_chunk_bytes()).map_err(
                |error| CliError::protocol("initialize terminal stream", error.to_string()),
            )?;
        match follow_once(
            &mut client,
            &mut state,
            binding,
            &mut receipt,
            &mut sanitizers,
            &arguments,
            output,
        )
        .await?
        {
            FollowOutcome::Exited => {
                return render_follow_receipt(
                    &receipt,
                    arguments.receipt.as_deref(),
                    output,
                );
            }
            FollowOutcome::Reconnect(_cause) => {
                drop(client);
                client = reconnect_for_follow(
                    endpoint,
                    timeout,
                    &arguments,
                    binding,
                    &mut receipt,
                    output,
                )
                .await?;
            }
            FollowOutcome::Interrupted => {
                complete_interrupt(
                    endpoint,
                    timeout,
                    &arguments,
                    binding,
                    &mut receipt,
                    output,
                )
                .await?;
                return Err(CliError::interrupted());
            }
        }
    }
}

async fn establish_attachment(
    client: &mut Client,
    binding: TerminalBinding,
    receipt: &mut FollowReceipt,
    arguments: &TerminalAttachArgs,
    activate: bool,
) -> Result<(), CliError> {
    let identity = attach_identity(receipt)?;
    let response =
        client.request(identity, AppRequestPayload::AttachTerminal(binding)).await?;
    let (observed, mode) = match response.payload() {
        AppResponsePayload::TerminalAttached(observed) => (*observed, "pty"),
        AppResponsePayload::TerminalPipeAttached(observed) => (*observed, "pipes"),
        _ => return response_error(response.payload(), "terminal attachment"),
    };
    if observed != binding {
        return Err(CliError::protocol(
            "validate terminal attachment",
            "daemon attached a different terminal binding",
        ));
    }
    if receipt.mode.as_deref().is_some_and(|stored| stored != mode) {
        return Err(CliError::protocol(
            "validate terminal attachment",
            "daemon changed the retained terminal attachment mode",
        ));
    }
    receipt.mode = Some(mode.to_owned());
    if activate {
        receipt.phase = FollowPhase::Active;
        receipt.unavailable_code = None;
        persist(arguments.receipt.as_deref(), receipt).await?;
    }
    Ok(())
}

fn announce_attachment(
    receipt: &FollowReceipt,
    binding: TerminalBinding,
    arguments: &TerminalAttachArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mode = receipt.mode.as_deref().unwrap_or("unknown");
    output.success(
        "terminal-attached",
        serde_json::json!({
            "attachment_id": hex(binding.attachment_id().as_bytes()),
            "process_id": hex(binding.process_id().as_bytes()),
            "originating_request_id": hex(binding.originating_request_id().as_bytes()),
            "session_id": receipt.session_id.as_str(),
            "mode": mode,
            "next_sequence": receipt.next_sequence,
            "next_offset": receipt.next_offset,
            "receipt": arguments.receipt.as_ref().map(|path| path.display().to_string()),
        }),
        &format!(
            "terminal attached ({mode}): attachment={} process={} originating-request={} session={}{}",
            hex(binding.attachment_id().as_bytes()),
            hex(binding.process_id().as_bytes()),
            hex(binding.originating_request_id().as_bytes()),
            receipt.session_id,
            if arguments.follow { "; following output" } else { "" },
        ),
    )
}

async fn follow_once(
    client: &mut Client,
    state: &mut TerminalState,
    binding: TerminalBinding,
    receipt: &mut FollowReceipt,
    sanitizers: &mut [TerminalSanitizer; 3],
    arguments: &TerminalAttachArgs,
    output: &Output,
) -> Result<FollowOutcome, CliError> {
    loop {
        let event = tokio::select! {
            result = client.read_event() => match result {
                Ok(event) => event,
                Err(error) if reconnectable(&error) || retryable_validation(&error) => {
                    return Ok(FollowOutcome::Reconnect(error));
                }
                Err(error) => return Err(error),
            },
            result = tokio::signal::ctrl_c() => {
                result.map_err(|error| {
                    CliError::connection("listen for interrupt", error.to_string())
                })?;
                return Ok(FollowOutcome::Interrupted);
            }
        };
        match client.reply_heartbeat(&event).await {
            Ok(true) => continue,
            Ok(false) => {}
            Err(error) if reconnectable(&error) || retryable_validation(&error) => {
                return Ok(FollowOutcome::Reconnect(error));
            }
            Err(error) => return Err(error),
        }
        match event.payload() {
            AppEventPayload::TerminalUnavailable(failed) if *failed == binding => {
                return Ok(FollowOutcome::Reconnect(CliError::remote_failure(
                    "follow terminal",
                    "daemon reported that the current attachment projection is unavailable",
                )));
            }
            AppEventPayload::TerminalOutput(chunk) if chunk.binding() == binding => {
                let before = state.stream_offsets();
                if let Err(error) = state.accept_output(chunk) {
                    return Ok(FollowOutcome::Reconnect(CliError::protocol(
                        "validate terminal output",
                        error.to_string(),
                    )));
                }
                if let Err(error) = reconcile_output(
                    chunk,
                    before,
                    state,
                    receipt,
                    sanitizers,
                    arguments.receipt.as_deref(),
                    output,
                )
                .await
                {
                    if retryable_validation(&error) {
                        return Ok(FollowOutcome::Reconnect(error));
                    }
                    return Err(error);
                }
            }
            AppEventPayload::TerminalOutputGap(gap) if gap.binding() == binding => {
                let before = state.stream_offsets();
                if let Err(error) = state.accept_output_gap(*gap) {
                    return Ok(FollowOutcome::Reconnect(CliError::protocol(
                        "validate terminal output gap",
                        error.to_string(),
                    )));
                }
                if let Err(error) = reconcile_gap(
                    *gap,
                    before,
                    state,
                    receipt,
                    sanitizers,
                    arguments.receipt.as_deref(),
                    output,
                )
                .await
                {
                    if retryable_validation(&error) {
                        return Ok(FollowOutcome::Reconnect(error));
                    }
                    return Err(error);
                }
            }
            AppEventPayload::TerminalExited(exit) if exit.binding() == binding => {
                if let Err(error) = state.exit(*exit) {
                    return Ok(FollowOutcome::Reconnect(CliError::protocol(
                        "validate terminal exit",
                        error.to_string(),
                    )));
                }
                if exit.final_offset() < receipt.next_offset {
                    return Ok(FollowOutcome::Reconnect(CliError::protocol(
                        "reconcile terminal exit",
                        "terminal exit regressed the durable output frontier",
                    )));
                }
                if exit.final_offset() > receipt.next_offset {
                    return Ok(FollowOutcome::Reconnect(CliError::protocol(
                        "reconcile terminal exit",
                        "terminal exit omitted output before its final fence",
                    )));
                }
                record_exit(receipt, *exit);
                snapshot_sanitizers(receipt, sanitizers);
                persist(arguments.receipt.as_deref(), receipt).await?;
                return Ok(FollowOutcome::Exited);
            }
            AppEventPayload::Diagnostic(diagnostic) => output.event(
                serde_json::json!({
                    "ok": true,
                    "kind": "diagnostic",
                    "message": diagnostic.as_str(),
                }),
                diagnostic.as_str(),
            )?,
            _ => {}
        }
    }
}

async fn reconcile_output(
    chunk: &peritus_app_protocol::TerminalOutput,
    before: [u64; 3],
    state: &TerminalState,
    receipt: &mut FollowReceipt,
    sanitizers: &mut [TerminalSanitizer; 3],
    path: Option<&Path>,
    output: &Output,
) -> Result<(), CliError> {
    let length = u64::try_from(chunk.bytes().len())
        .map_err(|_| CliError::runtime("reconcile terminal output", "chunk length overflow"))?;
    let end = chunk.offset().checked_add(length).ok_or_else(|| {
        CliError::protocol("reconcile terminal output", "chunk offset overflow")
    })?;
    let after = state.stream_offsets();
    if end <= receipt.next_offset {
        if !frontiers_at_or_before(after, receipt.stream_offsets) {
            return Err(CliError::protocol(
                "reconcile terminal output",
                "replayed stream frontier crossed the durable output frontier",
            ));
        }
        if end == receipt.next_offset {
            if after != receipt.stream_offsets {
                return Err(CliError::protocol(
                    "reconcile terminal output",
                    "replayed stream frontiers disagree at the durable output frontier",
                ));
            }
            receipt.next_sequence = state.next_output_sequence();
            persist(path, receipt).await?;
        }
        return Ok(());
    }
    if chunk.offset() > receipt.next_offset {
        return Err(CliError::protocol(
            "reconcile terminal output",
            "terminal output starts after the durable output frontier without a gap",
        ));
    }
    let overlap = receipt
        .next_offset
        .checked_sub(chunk.offset())
        .ok_or_else(|| CliError::protocol("reconcile terminal output", "invalid overlap"))?;
    let overlap = usize::try_from(overlap).map_err(|_| {
        CliError::protocol("reconcile terminal output", "output overlap does not fit memory")
    })?;
    if overlap > chunk.bytes().len() {
        return Err(CliError::protocol(
            "reconcile terminal output",
            "output overlap exceeds the replayed chunk",
        ));
    }
    let stream = stream_index(chunk.stream());
    let overlap_u64 = u64::try_from(overlap)
        .map_err(|_| CliError::runtime("reconcile terminal output", "overlap overflow"))?;
    for index in 0..3 {
        let expected = if index == stream {
            before[index].checked_add(overlap_u64).ok_or_else(|| {
                CliError::protocol("reconcile terminal output", "stream offset overflow")
            })?
        } else {
            before[index]
        };
        if expected != receipt.stream_offsets[index] {
            return Err(CliError::protocol(
                "reconcile terminal output",
                "replayed stream order disagrees with the durable output frontier",
            ));
        }
    }
    let visible = &chunk.bytes()[overlap..];
    if !visible.is_empty() {
        output.terminal_bytes(
            serde_json::json!({
                "ok": true,
                "kind": "terminal-output",
                "attachment_id": receipt.attachment_id.as_str(),
                "sequence": chunk.sequence(),
                "offset": receipt.next_offset,
                "stream": stream_name(chunk.stream()),
                "bytes_base64": BASE64.encode(visible),
                "replayed_prefix_bytes": overlap,
            }),
            visible,
            &mut sanitizers[stream],
        )?;
    }
    receipt.next_sequence = state.next_output_sequence();
    receipt.next_offset = state.next_output_offset();
    receipt.stream_offsets = after;
    snapshot_sanitizers(receipt, sanitizers);
    persist(path, receipt).await
}

async fn reconcile_gap(
    gap: peritus_app_protocol::TerminalOutputGap,
    before: [u64; 3],
    state: &TerminalState,
    receipt: &mut FollowReceipt,
    sanitizers: &mut [TerminalSanitizer; 3],
    path: Option<&Path>,
    output: &Output,
) -> Result<(), CliError> {
    let after = gap.stream_offsets();
    if gap.resume_offset() <= receipt.next_offset {
        if !frontiers_at_or_before(after, receipt.stream_offsets) {
            return Err(CliError::protocol(
                "reconcile terminal output gap",
                "replayed gap crossed a durable stream frontier",
            ));
        }
        if gap.resume_offset() == receipt.next_offset {
            if after != receipt.stream_offsets {
                return Err(CliError::protocol(
                    "reconcile terminal output gap",
                    "replayed gap stream frontiers disagree at the durable output frontier",
                ));
            }
            receipt.next_sequence = state.next_output_sequence();
            persist(path, receipt).await?;
        }
        return Ok(());
    }
    if gap.offset() > receipt.next_offset
        || !frontiers_between(before, receipt.stream_offsets, after)
    {
        return Err(CliError::protocol(
            "reconcile terminal output gap",
            "gap does not continue from the durable output frontier",
        ));
    }
    let missing = gap
        .resume_offset()
        .checked_sub(receipt.next_offset)
        .ok_or_else(|| {
            CliError::protocol("reconcile terminal output gap", "gap frontier regressed")
        })?;
    for index in 0..3 {
        if after[index] > receipt.stream_offsets[index] {
            sanitizers[index].discontinuity();
        }
    }
    output.event(
        serde_json::json!({
            "ok": true,
            "kind": "terminal-output-gap",
            "attachment_id": receipt.attachment_id.as_str(),
            "sequence": gap.sequence(),
            "offset": receipt.next_offset,
            "resume_offset": gap.resume_offset(),
            "missing_bytes": missing,
            "stdout_offset": after[0],
            "stderr_offset": after[1],
            "terminal_offset": after[2],
        }),
        &format!(
            "[{missing} terminal output bytes unavailable; resumed at offset {}]",
            gap.resume_offset(),
        ),
    )?;
    receipt.next_sequence = state.next_output_sequence();
    receipt.next_offset = state.next_output_offset();
    receipt.stream_offsets = after;
    snapshot_sanitizers(receipt, sanitizers);
    persist(path, receipt).await
}

async fn complete_interrupt(
    endpoint: &OsStr,
    timeout: Option<Duration>,
    arguments: &TerminalAttachArgs,
    binding: TerminalBinding,
    receipt: &mut FollowReceipt,
    _output: &Output,
) -> Result<(), CliError> {
    if !receipt.phase.is_control_prepared() {
        let identity = Client::new_request_identity()?;
        receipt.control_request_id = Some(hex(identity.request_id.as_bytes()));
        receipt.control_correlation_id = Some(hex(identity.correlation_id.as_bytes()));
        receipt.phase = match arguments.interrupt {
            TerminalInterruptAction::Detach => FollowPhase::DetachPrepared,
            TerminalInterruptAction::Cancel => FollowPhase::CancelPrepared,
        };
        persist(arguments.receipt.as_deref(), receipt).await?;
    }
    let identity = control_identity(receipt)?;
    let correlation = identity.correlation_id;
    let payload = match receipt.phase {
        FollowPhase::DetachPrepared => {
            AppRequestPayload::DetachTerminal(TerminalDetach::new(binding, correlation))
        }
        FollowPhase::CancelPrepared => AppRequestPayload::CancelTerminal(
            TerminalCancellation::new(binding, correlation),
        ),
        _ => {
            return Err(invalid_receipt(
                "terminal control reconciliation has no prepared control",
            ));
        }
    };

    let mut repaired_attachment = false;
    loop {
        let mut control = match reconnect(endpoint, &receipt.session_id, timeout).await {
            Ok(client) => client,
            Err(error) if reconnectable(&error) => {
                reconnect_delay().await;
                continue;
            }
            Err(error) => return Err(error),
        };
        match control.request(identity, payload.clone()).await {
            Ok(response) => match response.payload() {
                AppResponsePayload::Acknowledged(acknowledgement)
                    if acknowledgement.request_id() == identity.request_id =>
                {
                    receipt.phase = match receipt.phase {
                        FollowPhase::DetachPrepared => FollowPhase::Detached,
                        FollowPhase::CancelPrepared => FollowPhase::Cancelled,
                        _ => unreachable!("prepared phase was checked above"),
                    };
                    persist(arguments.receipt.as_deref(), receipt).await?;
                    return Ok(());
                }
                AppResponsePayload::Acknowledged(_) => {
                    return Err(CliError::protocol(
                        "reconcile terminal control",
                        "daemon acknowledged a different terminal control request",
                    ));
                }
                AppResponsePayload::Error(error)
                    if error.code() == AppErrorCode::TerminalState && !repaired_attachment =>
                {
                    match establish_attachment(
                        &mut control,
                        binding,
                        receipt,
                        arguments,
                        false,
                    )
                    .await
                    {
                        Ok(()) => {}
                        Err(error) if reconnectable(&error) => {
                            reconnect_delay().await;
                            continue;
                        }
                        Err(error) => return Err(error),
                    }
                    receipt.phase = match arguments.interrupt {
                        TerminalInterruptAction::Detach => FollowPhase::DetachPrepared,
                        TerminalInterruptAction::Cancel => FollowPhase::CancelPrepared,
                    };
                    persist(arguments.receipt.as_deref(), receipt).await?;
                    repaired_attachment = true;
                }
                _ => return response_error(response.payload(), "terminal control"),
            },
            Err(error) if reconnectable(&error) => {
                reconnect_delay().await;
            }
            Err(error) => return Err(error),
        }
    }
}

pub async fn input(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Option<Duration>,
    arguments: TerminalInputArgs,
    output: &Output,
) -> Result<(), CliError> {
    let binding = binding(&arguments.binding)?;
    let bootstrap = connect(endpoint, session, timeout).await?;
    let exact_session = bootstrap.context().session_id();
    let maximum = bootstrap.limits().max_terminal_chunk_bytes();
    drop(bootstrap);
    if maximum == 0 {
        return Err(CliError::protocol(
            "initialize terminal input",
            "negotiated terminal chunk bound is zero",
        ));
    }
    let sent = if arguments.input == OsStr::new("-") {
        let mut stdin = tokio::io::stdin();
        stream_input(
            endpoint,
            exact_session,
            timeout,
            binding,
            &mut stdin,
            None,
            maximum,
        )
        .await?
    } else {
        let path = PathBuf::from(arguments.input);
        let mut file = tokio::fs::File::open(&path).await.map_err(|error| {
            CliError::local_io("open terminal input", Some(path.clone()), error)
        })?;
        stream_input(
            endpoint,
            exact_session,
            timeout,
            binding,
            &mut file,
            Some(&path),
            maximum,
        )
        .await?
    };
    if sent == 0 {
        return Err(CliError::usage("terminal input must contain at least one byte"));
    }
    output.success(
        "terminal-input",
        serde_json::json!({
            "attachment_id": hex(binding.attachment_id().as_bytes()),
            "bytes": sent,
        }),
        &format!(
            "sent {sent} bytes to terminal {}",
            hex(binding.attachment_id().as_bytes()),
        ),
    )
}

async fn stream_input<R: AsyncRead + Unpin>(
    endpoint: &OsStr,
    session: SessionId,
    timeout: Option<Duration>,
    binding: TerminalBinding,
    reader: &mut R,
    path: Option<&Path>,
    maximum: usize,
) -> Result<u64, CliError> {
    let mut buffer = vec![0_u8; maximum];
    let mut sent = 0_u64;
    loop {
        let read = tokio::select! {
            result = reader.read(&mut buffer) => result.map_err(|error| {
                CliError::local_io("read terminal input", path.map(Path::to_path_buf), error)
            })?,
            result = tokio::signal::ctrl_c() => {
                result.map_err(|error| {
                    CliError::connection("listen for interrupt", error.to_string())
                })?;
                cancel_for_interrupt(endpoint, session, timeout, binding).await?;
                return Err(CliError::interrupted());
            }
        };
        if read == 0 {
            return Ok(sent);
        }
        let mut client = connect(endpoint, Some(session), timeout).await?;
        let negotiated = client.limits().max_terminal_chunk_bytes();
        if negotiated == 0 {
            return Err(CliError::protocol(
                "send terminal input",
                "reconnected terminal chunk bound is zero",
            ));
        }
        for bytes in buffer[..read].chunks(negotiated) {
            let input = TerminalInput::new(binding, bytes.to_vec(), negotiated)
                .map_err(|error| CliError::usage(error.to_string()))?;
            let identity = Client::new_request_identity()?;
            let response = tokio::select! {
                result = client.request(identity, AppRequestPayload::TerminalInput(input)) => {
                    result?
                }
                result = tokio::signal::ctrl_c() => {
                    result.map_err(|error| {
                        CliError::connection("listen for interrupt", error.to_string())
                    })?;
                    cancel_for_interrupt(endpoint, session, timeout, binding).await?;
                    return Err(CliError::interrupted());
                }
            };
            expect_ack(response.payload(), identity, "terminal input")?;
            sent = sent
                .checked_add(u64::try_from(bytes.len()).map_err(|_| {
                    CliError::runtime("count terminal input", "chunk length overflow")
                })?)
                .ok_or_else(|| {
                    CliError::runtime("count terminal input", "input byte count overflow")
                })?;
        }
    }
}

async fn cancel_for_interrupt(
    endpoint: &OsStr,
    session: SessionId,
    timeout: Option<Duration>,
    binding: TerminalBinding,
) -> Result<(), CliError> {
    let mut client = connect(endpoint, Some(session), timeout).await?;
    let identity = Client::new_request_identity()?;
    let cancellation = TerminalCancellation::new(binding, identity.correlation_id);
    let response =
        client.request(identity, AppRequestPayload::CancelTerminal(cancellation)).await?;
    expect_ack(response.payload(), identity, "terminal cancellation")
}

pub async fn resize(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Option<Duration>,
    arguments: TerminalResizeArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mut client = connect(endpoint, session, timeout).await?;
    let binding = binding(&arguments.binding)?;
    let resize = TerminalResize::new(
        binding,
        arguments.columns,
        arguments.rows,
        PORTABLE_NATIVE_TERMINAL_DIMENSION_MAX,
        PORTABLE_NATIVE_TERMINAL_DIMENSION_MAX,
    )
    .map_err(|error| {
        CliError::usage(format!(
            "{}; portable native terminal dimensions are at most {}x{}",
            error,
            PORTABLE_NATIVE_TERMINAL_DIMENSION_MAX,
            PORTABLE_NATIVE_TERMINAL_DIMENSION_MAX,
        ))
    })?;
    let identity = Client::new_request_identity()?;
    let response = client.request(identity, AppRequestPayload::TerminalResize(resize)).await?;
    expect_ack(response.payload(), identity, "terminal resize")?;
    output.success(
        "terminal-resized",
        serde_json::json!({
            "attachment_id": hex(binding.attachment_id().as_bytes()),
            "columns": arguments.columns,
            "rows": arguments.rows,
        }),
        &format!(
            "terminal {} resized to {}x{}",
            hex(binding.attachment_id().as_bytes()),
            arguments.columns,
            arguments.rows,
        ),
    )
}

pub async fn detach(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Option<Duration>,
    arguments: TerminalBindingArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mut client = connect(endpoint, session, timeout).await?;
    let binding = binding(&arguments)?;
    let identity = Client::new_request_identity()?;
    let detach = TerminalDetach::new(binding, identity.correlation_id);
    let response = client.request(identity, AppRequestPayload::DetachTerminal(detach)).await?;
    expect_ack(response.payload(), identity, "terminal detach")?;
    output.success(
        "terminal-detached",
        binding_json(binding),
        &format!(
            "terminal {} detached",
            hex(binding.attachment_id().as_bytes()),
        ),
    )
}

pub async fn cancel(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Option<Duration>,
    arguments: TerminalBindingArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mut client = connect(endpoint, session, timeout).await?;
    let binding = binding(&arguments)?;
    let identity = Client::new_request_identity()?;
    let cancellation = TerminalCancellation::new(binding, identity.correlation_id);
    let response =
        client.request(identity, AppRequestPayload::CancelTerminal(cancellation)).await?;
    expect_ack(response.payload(), identity, "terminal cancellation")?;
    output.success(
        "terminal-cancelled",
        binding_json(binding),
        &format!(
            "terminal {} cancelled",
            hex(binding.attachment_id().as_bytes()),
        ),
    )
}

async fn connect(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Option<Duration>,
) -> Result<Client, CliError> {
    Client::connect(
        endpoint,
        session,
        timeout,
        &[WellKnownProtocolFeature::TerminalStreaming],
    )
    .await
}

async fn reconnect(
    endpoint: &OsStr,
    session: &str,
    timeout: Option<Duration>,
) -> Result<Client, CliError> {
    connect(endpoint, Some(stored_session(session)?), timeout).await
}

async fn reconnect_for_follow(
    endpoint: &OsStr,
    timeout: Option<Duration>,
    arguments: &TerminalAttachArgs,
    binding: TerminalBinding,
    receipt: &mut FollowReceipt,
    output: &Output,
) -> Result<Client, CliError> {
    loop {
        let attempt = tokio::select! {
            result = reconnect(endpoint, &receipt.session_id, timeout) => result,
            result = tokio::signal::ctrl_c() => {
                result.map_err(|error| {
                    CliError::connection("listen for interrupt", error.to_string())
                })?;
                complete_interrupt(endpoint, timeout, arguments, binding, receipt, output).await?;
                return Err(CliError::interrupted());
            }
        };
        match attempt {
            Ok(client) => return Ok(client),
            Err(error) if reconnectable(&error) => {}
            Err(error) => return Err(error),
        }
        tokio::select! {
            () = tokio::time::sleep(RECONNECT_DELAY) => {}
            result = tokio::signal::ctrl_c() => {
                result.map_err(|error| {
                    CliError::connection("listen for interrupt", error.to_string())
                })?;
                complete_interrupt(endpoint, timeout, arguments, binding, receipt, output).await?;
                return Err(CliError::interrupted());
            }
        }
    }
}

async fn reconnect_delay() {
    tokio::time::sleep(RECONNECT_DELAY).await;
}

fn binding(arguments: &TerminalBindingArgs) -> Result<TerminalBinding, CliError> {
    let attachment = TerminalAttachmentId::new(arguments.attachment)
        .map_err(|_| CliError::usage("invalid --attachment identifier"))?;
    let process = ProcessId::new(arguments.process)
        .map_err(|_| CliError::usage("invalid --process identifier"))?;
    let request = RequestId::new(arguments.originating_request)
        .map_err(|_| CliError::usage("invalid --originating-request identifier"))?;
    Ok(TerminalBinding::new(attachment, process, request))
}

fn receipt_binding(receipt: &FollowReceipt) -> Result<TerminalBinding, CliError> {
    let attachment = TerminalAttachmentId::new(stored_id(
        &receipt.attachment_id,
        "attachment",
    )?)
    .map_err(|_| invalid_receipt("receipt contains an invalid attachment identity"))?;
    let process = ProcessId::new(stored_id(&receipt.process_id, "process")?)
        .map_err(|_| invalid_receipt("receipt contains an invalid process identity"))?;
    let request = RequestId::new(stored_id(
        &receipt.originating_request_id,
        "originating request",
    )?)
    .map_err(|_| invalid_receipt("receipt contains an invalid originating request identity"))?;
    Ok(TerminalBinding::new(attachment, process, request))
}

fn attach_identity(receipt: &FollowReceipt) -> Result<RequestIdentity, CliError> {
    restored_identity(
        &receipt.originating_request_id,
        &receipt.attach_correlation_id,
        "terminal attachment",
    )
}

fn control_identity(receipt: &FollowReceipt) -> Result<RequestIdentity, CliError> {
    restored_identity(
        receipt
            .control_request_id
            .as_deref()
            .ok_or_else(|| invalid_receipt("prepared control omitted its request identity"))?,
        receipt
            .control_correlation_id
            .as_deref()
            .ok_or_else(|| invalid_receipt("prepared control omitted its correlation identity"))?,
        "terminal control",
    )
}

fn restored_identity(
    request: &str,
    correlation: &str,
    operation: &str,
) -> Result<RequestIdentity, CliError> {
    let request = RequestId::new(stored_id(request, "request")?).map_err(|_| {
        invalid_receipt(&format!("receipt contains an invalid {operation} request identity"))
    })?;
    let correlation =
        CorrelationId::new(stored_id(correlation, "correlation")?).map_err(|_| {
            invalid_receipt(&format!(
                "receipt contains an invalid {operation} correlation identity",
            ))
        })?;
    Ok(RequestIdentity::new(request, correlation))
}

fn stored_session(value: &str) -> Result<SessionId, CliError> {
    SessionId::new(stored_id(value, "session")?)
        .map_err(|_| invalid_receipt("receipt contains an invalid session identity"))
}

fn stored_id(value: &str, field: &str) -> Result<[u8; 16], CliError> {
    parse_hex_id(value, field)
        .map_err(|_| invalid_receipt(&format!("receipt contains an invalid {field} identity")))
}

fn expect_ack(
    payload: &AppResponsePayload,
    identity: RequestIdentity,
    operation: &'static str,
) -> Result<(), CliError> {
    match payload {
        AppResponsePayload::Acknowledged(acknowledgement)
            if acknowledgement.request_id() == identity.request_id =>
        {
            Ok(())
        }
        AppResponsePayload::Acknowledged(_) => Err(CliError::protocol(
            operation,
            "daemon acknowledged a different request identity",
        )),
        _ => response_error(payload, operation),
    }
}

fn binding_json(binding: TerminalBinding) -> serde_json::Value {
    serde_json::json!({
        "attachment_id": hex(binding.attachment_id().as_bytes()),
        "process_id": hex(binding.process_id().as_bytes()),
        "originating_request_id": hex(binding.originating_request_id().as_bytes()),
    })
}

const fn stream_name(stream: TerminalStream) -> &'static str {
    match stream {
        TerminalStream::Stdout => "stdout",
        TerminalStream::Stderr => "stderr",
        TerminalStream::Terminal => "terminal",
    }
}

const fn stream_index(stream: TerminalStream) -> usize {
    match stream {
        TerminalStream::Stdout => 0,
        TerminalStream::Stderr => 1,
        TerminalStream::Terminal => 2,
    }
}

const fn frontiers_at_or_before(left: [u64; 3], right: [u64; 3]) -> bool {
    left[0] <= right[0] && left[1] <= right[1] && left[2] <= right[2]
}

const fn frontiers_between(
    before: [u64; 3],
    middle: [u64; 3],
    after: [u64; 3],
) -> bool {
    before[0] <= middle[0]
        && middle[0] <= after[0]
        && before[1] <= middle[1]
        && middle[1] <= after[1]
        && before[2] <= middle[2]
        && middle[2] <= after[2]
}

fn record_exit(receipt: &mut FollowReceipt, exit: TerminalExit) {
    let (kind, value, successful) = match exit.disposition() {
        TerminalExitDisposition::Code(code) => ("code", Some(code), code == 0),
        TerminalExitDisposition::Signal(signal) => ("signal", Some(signal), false),
        TerminalExitDisposition::Unknown => ("unknown", None, false),
    };
    receipt.phase = FollowPhase::Exited;
    receipt.next_sequence = exit.next_sequence();
    receipt.next_offset = exit.final_offset();
    receipt.exit_kind = Some(kind.to_owned());
    receipt.exit_value = value;
    receipt.exit_successful = Some(successful);
}

fn snapshot_sanitizers(
    receipt: &mut FollowReceipt,
    sanitizers: &[TerminalSanitizer; 3],
) {
    receipt.sanitizers = std::array::from_fn(|index| sanitizers[index].snapshot());
}

fn restored_sanitizers(
    receipt: &FollowReceipt,
) -> Result<[TerminalSanitizer; 3], CliError> {
    let mut values = Vec::with_capacity(3);
    for snapshot in &receipt.sanitizers {
        values.push(TerminalSanitizer::restore(snapshot).ok_or_else(|| {
            invalid_receipt("receipt contains an invalid terminal sanitizer frontier")
        })?);
    }
    values
        .try_into()
        .map_err(|_| invalid_receipt("receipt sanitizer frontier has the wrong width"))
}

fn follow_scope_fingerprint(endpoint: &OsStr, arguments: &TerminalAttachArgs) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"peritus/cli-terminal-follow-scope/v1\0");
    fingerprint_part(&mut hasher, endpoint.as_encoded_bytes());
    fingerprint_part(&mut hasher, &arguments.process);
    hasher.update([u8::from(arguments.follow)]);
    hasher.update([match arguments.interrupt {
        TerminalInterruptAction::Detach => 0,
        TerminalInterruptAction::Cancel => 1,
    }]);
    let digest: [u8; 32] = hasher.finalize().into();
    hex(&digest)
}

fn fingerprint_part(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn validate_follow_receipt(
    receipt: &FollowReceipt,
    scope: &str,
    requested_session: Option<SessionId>,
) -> Result<(), CliError> {
    if receipt.version != 1 || receipt.kind != "terminal-follow" {
        return Err(invalid_receipt(
            "receipt is not a supported terminal-follow receipt",
        ));
    }
    if receipt.scope_sha256 != scope {
        return Err(CliError::usage(
            "--receipt belongs to a different terminal, endpoint, or follow policy",
        ));
    }
    let session = stored_session(&receipt.session_id)?;
    if requested_session.is_some_and(|requested| requested != session) {
        return Err(CliError::usage(
            "--session does not match the durable session in --receipt",
        ));
    }
    let _ = receipt_binding(receipt)?;
    let _ = attach_identity(receipt)?;
    if receipt.stream_offsets.iter().try_fold(0_u64, |total, value| {
        total.checked_add(*value)
    }) != Some(receipt.next_offset)
    {
        return Err(invalid_receipt(
            "terminal receipt stream frontiers do not conserve the global offset",
        ));
    }
    if receipt.mode.as_deref().is_some_and(|mode| mode != "pty" && mode != "pipes") {
        return Err(invalid_receipt(
            "terminal receipt contains an invalid attachment mode",
        ));
    }
    if receipt.phase.is_control_prepared() {
        let _ = control_identity(receipt)?;
    } else if receipt.control_request_id.is_some() != receipt.control_correlation_id.is_some() {
        return Err(invalid_receipt(
            "terminal receipt contains a partial control identity",
        ));
    }
    if receipt.phase == FollowPhase::Exited
        && (receipt.exit_kind.is_none() || receipt.exit_successful.is_none())
    {
        return Err(invalid_receipt(
            "exited terminal receipt omitted its terminal disposition",
        ));
    }
    let _ = restored_sanitizers(receipt)?;
    Ok(())
}

fn render_follow_receipt(
    receipt: &FollowReceipt,
    path: Option<&Path>,
    output: &Output,
) -> Result<(), CliError> {
    match receipt.phase {
        FollowPhase::Exited => {
            let kind = receipt
                .exit_kind
                .as_deref()
                .ok_or_else(|| invalid_receipt("terminal exit omitted its kind"))?;
            let successful = receipt
                .exit_successful
                .ok_or_else(|| invalid_receipt("terminal exit omitted its success state"))?;
            output.event(
                serde_json::json!({
                    "ok": successful,
                    "kind": "terminal-exit",
                    "attachment_id": receipt.attachment_id.as_str(),
                    "disposition": kind,
                    "value": receipt.exit_value,
                    "next_sequence": receipt.next_sequence,
                    "final_offset": receipt.next_offset,
                    "receipt": path.map(|path| path.display().to_string()),
                }),
                &format!(
                    "terminal exited: {kind}={} ({} bytes)",
                    receipt
                        .exit_value
                        .map_or_else(|| "unavailable".to_owned(), |value| value.to_string()),
                    receipt.next_offset,
                ),
            )?;
            if successful {
                Ok(())
            } else {
                Err(CliError::remote_failure(
                    "follow terminal",
                    format!(
                        "process terminated with {kind}={:?}",
                        receipt.exit_value,
                    ),
                ))
            }
        }
        FollowPhase::Detached | FollowPhase::Cancelled => {
            let action = if receipt.phase == FollowPhase::Detached {
                "detached"
            } else {
                "cancelled"
            };
            output.success(
                &format!("terminal-{action}"),
                serde_json::json!({
                    "attachment_id": receipt.attachment_id.as_str(),
                    "process_id": receipt.process_id.as_str(),
                    "originating_request_id": receipt.originating_request_id.as_str(),
                    "next_sequence": receipt.next_sequence,
                    "next_offset": receipt.next_offset,
                    "receipt": path.map(|path| path.display().to_string()),
                }),
                &format!("terminal {} {action}", receipt.attachment_id),
            )
        }
        FollowPhase::Unavailable => Err(CliError::remote_failure(
            "resume terminal attachment",
            "the daemon authoritatively reported that the retained terminal is unavailable",
        )),
        _ => Err(invalid_receipt(
            "nonterminal terminal receipt cannot be rendered as complete",
        )),
    }
}

async fn persist(path: Option<&Path>, receipt: &FollowReceipt) -> Result<(), CliError> {
    match path {
        Some(path) => recovery::replace(path, receipt).await,
        None => Ok(()),
    }
}

fn reconnectable(error: &CliError) -> bool {
    error.category() == ExitCategory::Connection
}

fn retryable_validation(error: &CliError) -> bool {
    error.category() == ExitCategory::Protocol
}

fn invalid_receipt(detail: &str) -> CliError {
    CliError::runtime("validate operation receipt", detail.to_owned())
}

enum FollowOutcome {
    Exited,
    Reconnect(CliError),
    Interrupted,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
enum FollowPhase {
    Prepared,
    Active,
    DetachPrepared,
    CancelPrepared,
    Detached,
    Cancelled,
    Exited,
    Unavailable,
}

impl FollowPhase {
    const fn is_control_prepared(self) -> bool {
        matches!(self, Self::DetachPrepared | Self::CancelPrepared)
    }

    const fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Detached | Self::Cancelled | Self::Exited | Self::Unavailable
        )
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct FollowReceipt {
    version: u32,
    kind: String,
    scope_sha256: String,
    session_id: String,
    attachment_id: String,
    process_id: String,
    originating_request_id: String,
    attach_correlation_id: String,
    next_sequence: u64,
    next_offset: u64,
    stream_offsets: [u64; 3],
    sanitizers: [TerminalSanitizerSnapshot; 3],
    mode: Option<String>,
    phase: FollowPhase,
    control_request_id: Option<String>,
    control_correlation_id: Option<String>,
    exit_kind: Option<String>,
    exit_value: Option<i32>,
    exit_successful: Option<bool>,
    unavailable_code: Option<u64>,
}

use std::{ffi::OsStr, io::Read as _, path::PathBuf, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};
use peritus_app_protocol::{
    AppEventPayload, AppRequestPayload, AppResponsePayload, TerminalBinding, TerminalCancellation,
    TerminalDetach, TerminalExitDisposition, TerminalInput, TerminalResize, TerminalState,
    TerminalStream, WellKnownProtocolFeature,
};
use peritus_types::{ProcessId, SessionId};
use tokio::io::AsyncReadExt as _;

use crate::{
    args::{TerminalAttachArgs, TerminalBindingArgs, TerminalInputArgs, TerminalResizeArgs},
    client::Client,
    error::CliError,
    id::{generated_id, hex},
    operation::response_error,
    output::{Output, TerminalSanitizer},
};

pub async fn attach(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Duration,
    arguments: TerminalAttachArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mut client = connect(endpoint, session, timeout).await?;
    let process = ProcessId::new(arguments.process)
        .map_err(|_| CliError::usage("invalid --process identifier"))?;
    let identity = Client::new_request_identity()?;
    let attachment = peritus_app_protocol::TerminalAttachmentId::new(generated_id(b"terminal"))
        .map_err(|_| {
            CliError::runtime("create terminal attachment", "generated zero identifier")
        })?;
    let binding = TerminalBinding::new(attachment, process, identity.request_id);
    let response = client.request(identity, AppRequestPayload::AttachTerminal(binding)).await?;
    let AppResponsePayload::TerminalAttached(observed) = response.payload() else {
        return response_error(response.payload(), "terminal attachment");
    };
    if *observed != binding {
        return Err(CliError::protocol(
            "validate terminal attachment",
            "daemon attached a different terminal binding",
        ));
    }
    output.success(
        "terminal-attached",
        serde_json::json!({
            "attachment_id": hex(binding.attachment_id().as_bytes()),
            "process_id": hex(binding.process_id().as_bytes()),
            "originating_request_id": hex(binding.originating_request_id().as_bytes()),
            "session_id": hex(client.context().session_id().as_bytes()),
        }),
        &format!(
            "terminal attached: attachment={} process={} originating-request={} session={}{}",
            hex(binding.attachment_id().as_bytes()),
            hex(binding.process_id().as_bytes()),
            hex(binding.originating_request_id().as_bytes()),
            hex(client.context().session_id().as_bytes()),
            if arguments.follow { "; following output" } else { "" },
        ),
    )?;
    if arguments.follow { follow(&mut client, binding, output).await } else { Ok(()) }
}

async fn follow(
    client: &mut Client,
    binding: TerminalBinding,
    output: &Output,
) -> Result<(), CliError> {
    let mut state = TerminalState::new(binding, client.limits().max_terminal_chunk_bytes())
        .map_err(|error| CliError::protocol("initialize terminal stream", error.to_string()))?;
    let mut sanitizer = TerminalSanitizer::default();
    loop {
        let event = tokio::select! {
            result = client.read_event() => result?,
            result = tokio::signal::ctrl_c() => {
                result.map_err(|error| CliError::connection("listen for interrupt", error.to_string()))?;
                return Err(CliError::interrupted());
            }
        };
        if client.reply_heartbeat(&event).await? {
            continue;
        }
        match event.payload() {
            AppEventPayload::TerminalOutput(chunk) if chunk.binding() == binding => {
                state
                    .accept_output(chunk)
                    .map_err(|error| CliError::protocol("validate terminal output", error.to_string()))?;
                output.terminal_bytes(
                    serde_json::json!({
                        "ok": true,
                        "kind": "terminal-output",
                        "attachment_id": hex(binding.attachment_id().as_bytes()),
                        "sequence": chunk.sequence(),
                        "offset": chunk.offset(),
                        "stream": stream_name(chunk.stream()),
                        "bytes_base64": BASE64.encode(chunk.bytes()),
                    }),
                    chunk.bytes(),
                    &mut sanitizer,
                )?;
            }
            AppEventPayload::TerminalExited(exit) if exit.binding() == binding => {
                state
                    .exit(*exit)
                    .map_err(|error| CliError::protocol("validate terminal exit", error.to_string()))?;
                let (kind, value, successful) = match exit.disposition() {
                    TerminalExitDisposition::Code(code) => ("code", Some(code), code == 0),
                    TerminalExitDisposition::Signal(signal) => ("signal", Some(signal), false),
                    TerminalExitDisposition::Unknown => ("unknown", None, false),
                };
                output.event(
                    serde_json::json!({
                        "ok": successful,
                        "kind": "terminal-exit",
                        "attachment_id": hex(binding.attachment_id().as_bytes()),
                        "disposition": kind,
                        "value": value,
                        "next_sequence": exit.next_sequence(),
                        "final_offset": exit.final_offset(),
                    }),
                    &format!(
                        "terminal exited: {kind}={} ({} bytes)",
                        value.map_or_else(|| "unavailable".to_owned(), |value| value.to_string()),
                        exit.final_offset(),
                    ),
                )?;
                return if successful {
                    Ok(())
                } else {
                    Err(CliError::remote_failure(
                        "follow terminal",
                        format!("process terminated with {kind}={value:?}"),
                    ))
                };
            }
            AppEventPayload::Diagnostic(diagnostic) => output.event(
                serde_json::json!({ "ok": true, "kind": "diagnostic", "message": diagnostic.as_str() }),
                diagnostic.as_str(),
            )?,
            _ => {}
        }
    }
}

pub async fn input(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Duration,
    arguments: TerminalInputArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mut client = connect(endpoint, session, timeout).await?;
    let binding = binding(&arguments.binding)?;
    let maximum = client.limits().max_terminal_chunk_bytes();
    let sent = if arguments.input == OsStr::new("-") {
        let bytes = read_standard_input().await?;
        send_bytes(&mut client, binding, &bytes, maximum).await?
    } else {
        let path = PathBuf::from(arguments.input);
        let mut file = tokio::fs::File::open(&path).await.map_err(|error| {
            CliError::local_io("open terminal input", Some(path.clone()), error)
        })?;
        let mut sent = 0_u64;
        let mut buffer = vec![0_u8; maximum];
        loop {
            let read = file.read(&mut buffer).await.map_err(|error| {
                CliError::local_io("read terminal input", Some(path.clone()), error)
            })?;
            if read == 0 {
                break;
            }
            sent = sent
                .checked_add(send_bytes(&mut client, binding, &buffer[..read], maximum).await?)
                .ok_or_else(|| {
                    CliError::runtime("count terminal input", "input byte count overflow")
                })?;
        }
        sent
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
        &format!("sent {sent} bytes to terminal {}", hex(binding.attachment_id().as_bytes())),
    )
}

async fn send_bytes(
    client: &mut Client,
    binding: TerminalBinding,
    bytes: &[u8],
    maximum: usize,
) -> Result<u64, CliError> {
    let mut sent = 0_u64;
    for bytes in bytes.chunks(maximum) {
        if bytes.is_empty() {
            continue;
        }
        let input = TerminalInput::new(binding, bytes.to_vec(), maximum)
            .map_err(|error| CliError::usage(error.to_string()))?;
        let identity = Client::new_request_identity()?;
        let response = client.request(identity, AppRequestPayload::TerminalInput(input)).await?;
        expect_ack(response.payload())?;
        sent = sent
            .checked_add(
                u64::try_from(bytes.len()).map_err(|_| {
                    CliError::runtime("count terminal input", "chunk length overflow")
                })?,
            )
            .ok_or_else(|| {
                CliError::runtime("count terminal input", "input byte count overflow")
            })?;
    }
    Ok(sent)
}

pub async fn resize(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Duration,
    arguments: TerminalResizeArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mut client = connect(endpoint, session, timeout).await?;
    let binding = binding(&arguments.binding)?;
    let resize =
        TerminalResize::new(binding, arguments.columns, arguments.rows, u16::MAX, u16::MAX)
            .map_err(|error| CliError::usage(error.to_string()))?;
    let identity = Client::new_request_identity()?;
    let response = client.request(identity, AppRequestPayload::TerminalResize(resize)).await?;
    expect_ack(response.payload())?;
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
    timeout: Duration,
    arguments: TerminalBindingArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mut client = connect(endpoint, session, timeout).await?;
    let binding = binding(&arguments)?;
    let identity = Client::new_request_identity()?;
    let detach = TerminalDetach::new(binding, identity.correlation_id);
    let response = client.request(identity, AppRequestPayload::DetachTerminal(detach)).await?;
    expect_ack(response.payload())?;
    output.success(
        "terminal-detached",
        binding_json(binding),
        &format!("terminal {} detached", hex(binding.attachment_id().as_bytes())),
    )
}

pub async fn cancel(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Duration,
    arguments: TerminalBindingArgs,
    output: &Output,
) -> Result<(), CliError> {
    let mut client = connect(endpoint, session, timeout).await?;
    let binding = binding(&arguments)?;
    let identity = Client::new_request_identity()?;
    let cancellation = TerminalCancellation::new(binding, identity.correlation_id);
    let response =
        client.request(identity, AppRequestPayload::CancelTerminal(cancellation)).await?;
    expect_ack(response.payload())?;
    output.success(
        "terminal-cancelled",
        binding_json(binding),
        &format!("terminal {} cancelled", hex(binding.attachment_id().as_bytes())),
    )
}

async fn connect(
    endpoint: &OsStr,
    session: Option<SessionId>,
    timeout: Duration,
) -> Result<Client, CliError> {
    Client::connect(endpoint, session, timeout, &[WellKnownProtocolFeature::TerminalStreaming])
        .await
}

fn binding(arguments: &TerminalBindingArgs) -> Result<TerminalBinding, CliError> {
    let attachment = peritus_app_protocol::TerminalAttachmentId::new(arguments.attachment)
        .map_err(|_| CliError::usage("invalid --attachment identifier"))?;
    let process = ProcessId::new(arguments.process)
        .map_err(|_| CliError::usage("invalid --process identifier"))?;
    let request = peritus_app_protocol::RequestId::new(arguments.originating_request)
        .map_err(|_| CliError::usage("invalid --originating-request identifier"))?;
    Ok(TerminalBinding::new(attachment, process, request))
}

fn expect_ack(payload: &AppResponsePayload) -> Result<(), CliError> {
    match payload {
        AppResponsePayload::Acknowledged(_) => Ok(()),
        _ => response_error(payload, "operation acknowledgement"),
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

async fn read_standard_input() -> Result<Vec<u8>, CliError> {
    tokio::task::spawn_blocking(|| {
        let mut bytes = Vec::new();
        std::io::stdin().lock().read_to_end(&mut bytes).map(|_| bytes)
    })
    .await
    .map_err(|error| CliError::runtime("join standard-input reader", error.to_string()))?
    .map_err(|error| CliError::local_io("read standard input", None, error))
}

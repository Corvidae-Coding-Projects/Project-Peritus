//! Bounded official account-runtime metadata queries, with no inference or credential custody.

use super::{DiscoveredModel, MAX_CATALOG_MODELS, parse_runtime_models, unavailable};
use crate::{CancellationToken, ProviderCoreError};
use serde_json::Value;
use std::{collections::BTreeSet, path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, ChildStdout, Command},
};

const MAX_FRAME: usize = 4 * 1024 * 1024;
const MAX_TOTAL: usize = 16 * 1024 * 1024;

/// Credential-owning official executable protocol used solely for catalog discovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AccountCatalog {
    /// Codex app-server initialize and paginated model/list.
    Codex,
    /// Claude streaming SDK control initialization, whose response includes supported models.
    Claude,
}

/// Queries a pinned official executable and always terminates/reaps this metadata-only process.
///
/// No user prompt or inference request is sent, no account token is read, and repository-local
/// configuration is excluded by a fresh neutral working directory.
///
/// # Errors
/// Returns an actionable unsupported/authentication/protocol/timeout failure without raw output.
pub async fn discover_account_models(
    executable: &Path,
    kind: AccountCatalog,
    cancellation: &CancellationToken,
) -> Result<Vec<DiscoveredModel>, ProviderCoreError> {
    if cancellation.is_cancelled() {
        return Err(ProviderCoreError::cancelled("model_catalog"));
    }
    let directory = tempfile::Builder::new()
        .prefix("peritus-model-catalog-")
        .tempdir()
        .map_err(|_| unavailable("cannot create isolated model-discovery directory"))?;
    let mut command = Command::new(executable);
    command
        .current_dir(directory.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    match kind {
        AccountCatalog::Codex => {
            command.arg("app-server");
        }
        AccountCatalog::Claude => {
            command.args([
                "--print",
                "--input-format",
                "stream-json",
                "--output-format",
                "stream-json",
                "--verbose",
                "--tools",
                "",
                "--strict-mcp-config",
                "--mcp-config",
                "{\"mcpServers\":{}}",
                "--disable-slash-commands",
                "--no-session-persistence",
                "--settings",
                "{\"disableAllHooks\":true}",
            ]);
        }
    }
    let mut child = command.spawn().map_err(|error| spawn_error(&error))?;
    let result = match (child.stdin.take(), child.stdout.take()) {
        (Some(mut input), Some(output)) => {
            let operation =
                tokio::time::timeout(Duration::from_secs(30), query(&mut input, output, kind));
            match crate::cancel_first(cancellation, operation).await {
                None => Err(ProviderCoreError::cancelled("model_catalog")),
                Some(Err(_)) => Err(unavailable("official runtime model discovery timed out")),
                Some(Ok(result)) => result,
            }
        }
        _ => Err(unavailable("official runtime model-discovery pipes are unavailable")),
    };
    // This metadata process has no delegated tool children or live inference to preserve.
    let _ = child.kill().await;
    let _ = child.wait().await;
    result
}

fn spawn_error(error: &std::io::Error) -> ProviderCoreError {
    #[cfg(unix)]
    if error.raw_os_error() == Some(26) {
        return unavailable(
            "official runtime executable is busy; retry after installation finishes",
        );
    }
    unavailable(match error.kind() {
        std::io::ErrorKind::NotFound => "official runtime executable is missing",
        std::io::ErrorKind::PermissionDenied => "permission denied starting official runtime",
        std::io::ErrorKind::WouldBlock => {
            "operating system process capacity is temporarily unavailable"
        }
        _ => "cannot start official runtime for model discovery",
    })
}

async fn query(
    input: &mut ChildStdin,
    output: ChildStdout,
    kind: AccountCatalog,
) -> Result<Vec<DiscoveredModel>, ProviderCoreError> {
    let mut reader = BufReader::new(output);
    let mut total = 0;
    if kind == AccountCatalog::Claude {
        send(input, metadata(r#"{"type":"control_request","request_id":"peritus-models","request":{"subtype":"initialize","hooks":null}}"#)?).await?;
        for _ in 0..64 {
            let frame = read_frame(&mut reader, &mut total).await?;
            if frame.get("type").and_then(Value::as_str) == Some("control_response")
                && frame.pointer("/response/request_id").and_then(Value::as_str)
                    == Some("peritus-models")
            {
                if frame.pointer("/response/subtype").and_then(Value::as_str) == Some("error") {
                    return Err(unavailable(
                        "official Claude runtime rejected model discovery; check sign-in and runtime version",
                    ));
                }
                return parse_runtime_models(frame.pointer("/response/response/models")
                    .ok_or_else(|| unavailable("official Claude runtime did not advertise models; select an explicit model ID"))?);
            }
        }
        return Err(unavailable("official Claude initialization exceeded its frame bound"));
    }
    send(input, metadata(r#"{"id":1,"method":"initialize","params":{"clientInfo":{"name":"peritus-model-catalog","version":"1"}}}"#)?).await?;
    let _ = response(&mut reader, &mut total, 1).await?;
    send(input, metadata(r#"{"method":"initialized","params":{}}"#)?).await?;
    let mut cursor = Value::Null;
    let mut cursors = BTreeSet::new();
    let mut models = Vec::new();
    for id in 2..18 {
        let mut request = metadata(
            r#"{"id":0,"method":"model/list","params":{"limit":1000,"cursor":null,"includeHidden":false}}"#,
        )?;
        request["id"] = Value::from(id);
        request["params"]["cursor"] = cursor.clone();
        send(input, request).await?;
        let result = response(&mut reader, &mut total, id).await?;
        models.extend(parse_runtime_models(
            result
                .get("data")
                .ok_or_else(|| unavailable("official Codex runtime did not advertise models"))?,
        )?);
        if models.len() > MAX_CATALOG_MODELS {
            return Err(unavailable("official runtime catalog exceeds its entry bound"));
        }
        match result.get("nextCursor").and_then(Value::as_str).filter(|value| !value.is_empty()) {
            None => return Ok(models),
            Some(next) if next.len() <= 4096 && cursors.insert(next.to_owned()) => {
                cursor = Value::from(next);
            }
            Some(_) => {
                return Err(unavailable("official runtime returned an invalid pagination cursor"));
            }
        }
    }
    Err(unavailable("official runtime model catalog exceeds its page bound"))
}

fn metadata(text: &str) -> Result<Value, ProviderCoreError> {
    serde_json::from_str(text).map_err(|_| unavailable("invalid model-discovery request metadata"))
}

async fn response(
    reader: &mut BufReader<ChildStdout>,
    total: &mut usize,
    id: u32,
) -> Result<Value, ProviderCoreError> {
    for _ in 0..64 {
        let value = read_frame(reader, total).await?;
        if value.get("id").and_then(Value::as_u64) == Some(u64::from(id)) {
            if value.get("error").is_some() {
                return Err(unavailable(
                    "official Codex runtime rejected model discovery; check sign-in and runtime version",
                ));
            }
            return value
                .get("result")
                .cloned()
                .ok_or_else(|| unavailable("official runtime returned no catalog result"));
        }
    }
    Err(unavailable("official runtime metadata response exceeded its frame bound"))
}

async fn send(input: &mut ChildStdin, value: Value) -> Result<(), ProviderCoreError> {
    let mut bytes = value.to_string().into_bytes();
    bytes.push(b'\n');
    input
        .write_all(&bytes)
        .await
        .map_err(|_| unavailable("official runtime model-discovery input closed"))?;
    input.flush().await.map_err(|_| unavailable("official runtime model-discovery input closed"))
}

async fn read_frame(
    reader: &mut BufReader<ChildStdout>,
    total: &mut usize,
) -> Result<Value, ProviderCoreError> {
    let mut frame = Vec::new();
    loop {
        let available = reader
            .fill_buf()
            .await
            .map_err(|_| unavailable("official runtime model-discovery output failed"))?;
        if available.is_empty() {
            return Err(unavailable("official runtime ended before model discovery completed"));
        }
        let end = available.iter().position(|byte| *byte == b'\n').map(|index| index + 1);
        let length = end.unwrap_or(available.len());
        *total = total.saturating_add(length);
        if frame.len().saturating_add(length) > MAX_FRAME || *total > MAX_TOTAL {
            return Err(unavailable("official runtime catalog exceeds its output bound"));
        }
        frame.extend_from_slice(&available[..length]);
        reader.consume(length);
        if end.is_some() {
            break;
        }
    }
    serde_json::from_slice(&frame)
        .map_err(|_| unavailable("official runtime returned malformed model metadata"))
}

//! Hardened authentication and isolated one-turn process projections.

use std::fs::OpenOptions;
use std::io::{Read as _, Write as _};
use std::path::Path;
use std::time::Duration;

use peritus_model_protocol::ModelRequest;
use peritus_provider_core::{
    BoxFuture, CancellationToken, EnvironmentName, ProcessLimits, ProcessRequest, ProcessTransport,
    ProviderCoreError,
};

use super::super::CodexRuntimeConfig;
use super::super::output::DecodeFailure;
use super::super::request::RuntimeRequest;

pub(super) struct CacheAnchor {
    pub(super) thread: String,
    // Creation time of the reusable routing frame, not the current task or observation.
    pub(super) created_seconds: f64,
    pub(super) available: std::sync::atomic::AtomicBool,
}

pub(super) struct TurnOutput {
    pub(super) process: peritus_provider_core::ProcessOutput,
    pub(super) final_message: Result<String, DecodeFailure>,
}

pub(super) struct TurnContext<'a> {
    pub(super) config: &'a CodexRuntimeConfig,
    pub(super) transport: &'a dyn ProcessTransport,
    pub(super) working_directory: &'a Path,
    pub(super) cache_anchor: Option<&'a CacheAnchor>,
    pub(super) cancellation: &'a CancellationToken,
    pub(super) progress: std::sync::Arc<super::super::live::Progress>,
}

pub(super) const DISABLED_NATIVE_FEATURES: &[&str] = &[
    "shell_tool",
    "unified_exec",
    "apps",
    "plugins",
    "multi_agent",
    "browser_use",
    "computer_use",
    "image_generation",
    "view_image",
    "hooks",
    "skill_search",
    "skill_mcp_dependency_install",
    "tool_call_mcp_elicitation",
    "request_permissions_tool",
    "code_mode",
    "sleep_tool",
    "tool_suggest",
    // A per-inference timestamp ahead of the host request invalidates its entire cache prefix.
    // Peritus supplies task context; the token router must not inject a second changing context.
    "current_time_reminder",
    "memories",
    "context_management",
];

pub(super) fn require_authenticated<'a>(
    config: &'a CodexRuntimeConfig,
    transport: &'a dyn ProcessTransport,
    cancellation: &'a CancellationToken,
) -> BoxFuture<'a, Result<(), ProviderCoreError>> {
    Box::pin(async move {
        let process = ProcessRequest::new(
            config.executable().process_executable().clone(),
            vec!["login".to_owned(), "status".to_owned()],
            Vec::new(),
            None,
            isolated_environment()?,
            authentication_limits()?,
        )?;
        let output = transport.run(process, cancellation).await?;
        if !output.exit().success() {
            return Err(not_authenticated());
        }
        Ok(())
    })
}

pub(super) async fn run_turn(
    context: TurnContext<'_>,
    request: &ModelRequest,
    runtime: &RuntimeRequest,
) -> Result<TurnOutput, ProviderCoreError> {
    // Keep the native environment prefix stable across requests. The previous fresh cwd
    // changed Codex's environment instructions ahead of the entire reusable host prompt.
    // Artifacts remain private and per-turn; native tools and session persistence stay off.
    if let Some(anchor) = context.cache_anchor {
        if let Some(output) = super::cached::run_turn(
            context.config,
            context.transport,
            runtime,
            context.working_directory,
            anchor,
            context.cancellation,
            context.progress.clone(),
        )
        .await?
        {
            return Ok(output);
        }
        // No model turn was submitted. A stale/unsupported routing anchor must
        // not strand otherwise usable account requests or replay accepted work.
        anchor.available.store(false, std::sync::atomic::Ordering::Release);
        eprintln!("peritus: Codex cache routing unavailable; using isolated turns");
        context.progress.begin_transport();
    }
    let directory =
        tempfile::tempdir_in(context.working_directory).map_err(|_| temporary_failure())?;
    let mut schema =
        tempfile::NamedTempFile::new_in(directory.path()).map_err(|_| temporary_failure())?;
    schema.write_all(&runtime.schema).map_err(|_| temporary_failure())?;
    schema.flush().map_err(|_| temporary_failure())?;
    let schema_path = path_argument(schema.path())?;
    let image_paths = write_images(directory.path(), runtime.images())?;
    let final_path = directory.path().join("final-response.json");
    let arguments = arguments(
        request.model().as_str(),
        runtime.reasoning_effort(),
        runtime.cyber_access_program(),
        schema_path,
        path_argument(&final_path)?,
        &image_paths,
    );
    let process = ProcessRequest::new(
        context.config.executable().process_executable().clone(),
        arguments,
        runtime.prompt.clone(),
        Some(context.working_directory.to_path_buf()),
        isolated_environment()?,
        context.config.process_limits(),
    )?
    .with_stdout_observer(std::sync::Arc::new(move |bytes| context.progress.observe(bytes)));
    let process = context.transport.run(process, context.cancellation).await?;
    let final_message = read_final(&final_path);
    Ok(TurnOutput { process, final_message })
}

fn read_final(path: &Path) -> Result<String, DecodeFailure> {
    let metadata = std::fs::symlink_metadata(path).map_err(|_| DecodeFailure::InvalidEnvelope)?;
    let maximum = peritus_model_protocol::ProtocolLimits::PRODUCTION.max_text_bytes();
    if !metadata.is_file() || metadata.len() > maximum as u64 {
        return Err(DecodeFailure::OutputLimit);
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|_| DecodeFailure::InvalidEnvelope)?
        .take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| DecodeFailure::InvalidEnvelope)?;
    if bytes.len() > maximum {
        return Err(DecodeFailure::OutputLimit);
    }
    String::from_utf8(bytes).map_err(|_| DecodeFailure::InvalidEnvelope)
}

/// Create a routing-only official-client anchor, without asking a model or storing a task.
/// Ephemeral forks of that anchor share cache affinity, not conversation or tool authority.
pub(super) async fn create_cache_anchor(
    config: &CodexRuntimeConfig,
    transport: &dyn ProcessTransport,
    directory: &Path,
    cancellation: &CancellationToken,
) -> Result<CacheAnchor, ProviderCoreError> {
    let limits = ProcessLimits::new(1024 * 1024, 1024 * 1024, 64 * 1024, Duration::from_secs(30))?;
    let help = ProcessRequest::new(
        config.executable().process_executable().clone(),
        vec!["exec".into(), "fork".into(), "--help".into()],
        Vec::new(),
        Some(directory.to_owned()),
        isolated_environment()?,
        limits,
    )?;
    if !transport.run(help, cancellation).await?.exit().success() {
        return Err(temporary_failure());
    }
    let input = [
        serde_json::json!({"id":1,"method":"initialize","params":{
            "clientInfo":{"name":"peritus-cache-router","version":env!("CARGO_PKG_VERSION")},
            "capabilities":{"experimentalApi":true}
        }}),
        serde_json::json!({"method":"initialized","params":{}}),
        serde_json::json!({"id":2,"method":"thread/start","params":{
            "model":config.profile().model().as_str(),"cwd":path_argument(directory)?,
            "sandbox":"read-only","approvalPolicy":"never","ephemeral":false,"historyMode":"legacy",
            "serviceName":"peritus-cache-router",
            "baseInstructions":"Peritus is the sole host agent, policy authority, and owner of conversation state. This thread is only a cache routing anchor. Each ephemeral fork receives one complete Peritus request; follow its host policy and output contract. Native tools must never execute.",
            "developerInstructions":""
        }}),
    ].into_iter().map(|value| value.to_string()).collect::<Vec<_>>().join("\n") + "\n";
    let close = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let observed = close.clone();
    let messages = std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
    let followup = messages.clone();
    let initialized = std::sync::Arc::new(std::sync::atomic::AtomicU8::new(0));
    let arguments = cached_arguments();
    let process = ProcessRequest::new(
        config.executable().process_executable().clone(),
        arguments,
        input.into_bytes(),
        Some(directory.to_owned()),
        isolated_environment()?,
        limits,
    )?
    .with_stdin_close_signal(close)
    .with_stdin_messages(messages)
    .with_stdout_observer(std::sync::Arc::new(move |bytes| {
        for line in bytes.split(|byte| *byte == b'\n') {
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(line) else { continue };
            let stage = initialized.load(std::sync::atomic::Ordering::Acquire);
            let anchor = bytes.split(|byte| *byte == b'\n').find_map(|line| {
                let value: serde_json::Value = serde_json::from_slice(line).ok()?;
                (value["id"] == 2).then(|| value["result"]["thread"]["id"].as_str().map(str::to_owned)).flatten()
            });
            let next = if value["id"] == 2 && value.get("result").is_some() && stage == 0 {
                anchor.as_ref().map(|id| serde_json::json!({"id":3,"method":"thread/inject_items","params":{"threadId":id,"items":[{"type":"message","role":"developer","content":[{"type":"input_text","text":"Peritus cache routing context initialized. No task has been supplied."}]}]}}))
            } else if value["id"] == 3 && value.get("result").is_some() && stage == 1 {
                anchor.as_ref().map(|id| serde_json::json!({"id":4,"method":"thread/archive","params":{"threadId":id}}))
            } else if value["id"] == 4 && value.get("result").is_some() && stage == 2 {
                anchor.as_ref().map(|id| serde_json::json!({"id":5,"method":"thread/unarchive","params":{"threadId":id}}))
            } else { None };
            if let Some(next) = next && let Ok(mut messages) = followup.lock() {
                initialized.store(stage + 1, std::sync::atomic::Ordering::Release);
                messages.push_back((next.to_string() + "\n").into_bytes());
            }
            if value["id"] == 5 || value.get("error").is_some() {
                observed.store(true, std::sync::atomic::Ordering::Release);
            }
        }
    }));
    let output = transport.run(process, cancellation).await?;
    if !output.exit().success() {
        return Err(temporary_failure());
    }
    let archived = output.stdout().split(|byte| *byte == b'\n').any(|line| {
        serde_json::from_slice::<serde_json::Value>(line)
            .is_ok_and(|value| value["id"] == 5 && value.get("result").is_some())
    });
    if !archived {
        return Err(temporary_failure());
    }
    for line in output.stdout().split(|byte| *byte == b'\n') {
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(line)
            && value["id"] == 2
            && let Some(id) = value["result"]["thread"]["id"].as_str()
            && id.len() == 36
            && id.bytes().all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
        {
            return Ok(CacheAnchor {
                thread: id.to_owned(),
                available: std::sync::atomic::AtomicBool::new(true),
                created_seconds: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|_| temporary_failure())?
                    .as_secs_f64(),
            });
        }
    }
    Err(temporary_failure())
}

// App-server otherwise loads user Codex memory/compaction settings a second time.
// Peritus already owns that context and all tool execution.
pub(super) fn cached_arguments() -> Vec<String> {
    let mut arguments = vec![
        "app-server".into(),
        "--listen".into(),
        "stdio://".into(),
        "--enable".into(),
        "skip_host_skill_discovery".into(),
    ];
    for feature in DISABLED_NATIVE_FEATURES {
        arguments.extend(["--disable".into(), (*feature).into()]);
    }
    for config in [
        "mcp_servers={}",
        "web_search=\"disabled\"",
        "tools.update_plan.enabled=false",
        "tools.experimental_request_user_input.enabled=false",
    ] {
        arguments.extend(["--config".into(), config.into()]);
    }
    arguments
}

fn arguments(
    model: &str,
    reasoning_effort: &str,
    cyber_access_program: Option<&str>,
    schema_path: String,
    final_path: String,
    image_paths: &[String],
) -> Vec<String> {
    let mut values = vec![
        "exec".to_owned(),
        "--json".to_owned(),
        "--ephemeral".to_owned(),
        "--ignore-user-config".to_owned(),
        "--ignore-rules".to_owned(),
        "--skip-git-repo-check".to_owned(),
        "--sandbox".to_owned(),
        "read-only".to_owned(),
        "--color".to_owned(),
        "never".to_owned(),
        "--model".to_owned(),
        model.to_owned(),
        "--config".to_owned(),
        format!("model_reasoning_effort=\"{reasoning_effort}\""),
        "--config".to_owned(),
        "model_reasoning_summary=\"auto\"".to_owned(),
        "--config".to_owned(),
        "tools.update_plan.enabled=false".to_owned(),
        "--config".to_owned(),
        "tools.experimental_request_user_input.enabled=false".to_owned(),
        "--config".to_owned(),
        "web_search=\"disabled\"".to_owned(),
    ];
    for feature in DISABLED_NATIVE_FEATURES {
        values.push("--disable".to_owned());
        values.push((*feature).to_owned());
    }
    if let Some(program) = cyber_access_program {
        values.push("--cyber-access-program".to_owned());
        values.push(program.to_owned());
    }
    for path in image_paths {
        values.push("--image".to_owned());
        values.push(path.clone());
    }
    values.push("--output-schema".to_owned());
    values.push(schema_path);
    values.push("--output-last-message".to_owned());
    values.push(final_path);
    values.push("-".to_owned());
    values
}

fn write_images(
    directory: &Path,
    images: &[super::super::request::RuntimeImage],
) -> Result<Vec<String>, ProviderCoreError> {
    images
        .iter()
        .enumerate()
        .map(|(index, image)| {
            let extension = match image.media_type.as_str() {
                "image/png" => "png",
                "image/jpeg" => "jpg",
                "image/webp" => "webp",
                "image/gif" => "gif",
                _ => return Err(unsupported_image_type()),
            };
            let path = directory.join(format!("input-image-{index:04}.{extension}"));
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|_| temporary_failure())?;
            file.write_all(&image.bytes).map_err(|_| temporary_failure())?;
            file.flush().map_err(|_| temporary_failure())?;
            path_argument(&path)
        })
        .collect()
}

pub(super) fn isolated_environment() -> Result<Vec<EnvironmentName>, ProviderCoreError> {
    [
        "OPENAI_API_KEY",
        "CODEX_API_KEY",
        "CODEX_ACCESS_TOKEN",
        "OPENAI_BASE_URL",
        "OPENAI_API_BASE",
        "OPENAI_ORG_ID",
        "OPENAI_ORGANIZATION",
        "OPENAI_PROJECT_ID",
    ]
    .into_iter()
    .map(|name| EnvironmentName::new(name.to_owned()))
    .collect()
}

const fn authentication_limits() -> Result<ProcessLimits, ProviderCoreError> {
    ProcessLimits::new(1, 64 * 1024, 64 * 1024, Duration::from_secs(10))
}

fn path_argument(path: &Path) -> Result<String, ProviderCoreError> {
    path.to_str().map(str::to_owned).ok_or_else(temporary_failure)
}

const fn temporary_failure() -> ProviderCoreError {
    ProviderCoreError::configuration(
        "codex_runtime_tempfile",
        "private temporary runtime state could not be created",
    )
}

const fn unsupported_image_type() -> ProviderCoreError {
    ProviderCoreError::invalid_request(
        "codex_runtime_image_type",
        "Codex runtime supports inline PNG, JPEG, WebP, and GIF images",
    )
}

const fn not_authenticated() -> ProviderCoreError {
    ProviderCoreError::credential(
        "Codex is not authenticated; run `codex login` then `codex login status`",
    )
}

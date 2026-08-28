//! Hardened authentication and isolated one-turn process projections.

use std::fs::OpenOptions;
use std::io::Write as _;
use std::path::Path;
use std::time::Duration;

use peritus_model_protocol::ModelRequest;
use peritus_provider_core::{
    BoxFuture, CancellationToken, EnvironmentName, ProcessLimits, ProcessRequest, ProcessTransport,
    ProviderCoreError,
};

use super::super::CodexRuntimeConfig;
use super::super::request::RuntimeRequest;

const DISABLED_NATIVE_FEATURES: &[&str] = &[
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
    config: &CodexRuntimeConfig,
    transport: &dyn ProcessTransport,
    request: &ModelRequest,
    runtime: &RuntimeRequest,
    cancellation: &CancellationToken,
) -> Result<peritus_provider_core::ProcessOutput, ProviderCoreError> {
    let directory = tempfile::tempdir().map_err(|_| temporary_failure())?;
    let mut schema =
        tempfile::NamedTempFile::new_in(directory.path()).map_err(|_| temporary_failure())?;
    schema.write_all(&runtime.schema).map_err(|_| temporary_failure())?;
    schema.flush().map_err(|_| temporary_failure())?;
    let schema_path = path_argument(schema.path())?;
    let image_paths = write_images(directory.path(), runtime.images())?;
    let process = ProcessRequest::new(
        config.executable().process_executable().clone(),
        arguments(request.model().as_str(), schema_path, &image_paths),
        runtime.prompt.clone(),
        Some(directory.path().to_path_buf()),
        isolated_environment()?,
        config.process_limits(),
    )?;
    transport.run(process, cancellation).await
}

fn arguments(model: &str, schema_path: String, image_paths: &[String]) -> Vec<String> {
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
        "model_reasoning_effort=\"low\"".to_owned(),
    ];
    for feature in DISABLED_NATIVE_FEATURES {
        values.push("--disable".to_owned());
        values.push((*feature).to_owned());
    }
    for path in image_paths {
        values.push("--image".to_owned());
        values.push(path.clone());
    }
    values.push("--output-schema".to_owned());
    values.push(schema_path);
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

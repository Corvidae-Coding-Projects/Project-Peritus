//! Narrow production-tool qualification used by release resilience campaigns.

use std::fs;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use peritus_agent::DeveloperToolExecutor as _;
use peritus_model_protocol::{
    CanonicalJson, CompletedToolCall, JsonBounds, ProtocolLimits, ToolCallId, ToolName,
};
use peritus_types::Sha256Digest;
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};

use crate::developer_tools::{WorkspaceDeveloperTools, WorkspaceOwnership};
use crate::{ProductRunnerError, ProductRunnerErrorKind};

/// Direct observation from one real bounded product `run_command` failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolProcessFailureObservation {
    exit_code: i32,
    stdout_sha256: Sha256Digest,
    receipt_bytes: u64,
}

impl ToolProcessFailureObservation {
    /// Returns the nonzero child exit code.
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        self.exit_code
    }

    /// Returns the SHA-256 of bounded child standard output.
    #[must_use]
    pub const fn stdout_sha256(&self) -> Sha256Digest {
        self.stdout_sha256
    }

    /// Returns durable effect-receipt bytes written for the command.
    #[must_use]
    pub const fn receipt_bytes(&self) -> u64 {
        self.receipt_bytes
    }
}

/// Exercises the same grounded, receipt-backed command tool used by ordinary coding runs.
///
/// # Errors
///
/// Returns a typed product-runner failure if setup, grounding, execution, result decoding, or
/// durable receipt verification differs from the qualification contract.
#[allow(
    clippy::too_many_lines,
    reason = "the release probe keeps setup, execution, and exact retained observations together"
)]
pub fn qualify_tool_process_failure(
    workspace: &Path,
    receipt_path: PathBuf,
    executable: &Path,
    dependency: &str,
    attempt: u16,
) -> Result<ToolProcessFailureObservation, ProductRunnerError> {
    fs::create_dir_all(workspace).map_err(|error| qualification(error.to_string()))?;
    let marker = workspace.join("peritus-qualification.marker");
    if !marker.exists() {
        fs::write(&marker, b"production tool qualification\n")
            .map_err(|error| qualification(error.to_string()))?;
    }
    let executable = executable
        .to_str()
        .ok_or_else(|| qualification("candidate executable path is not Unicode"))?;
    let scope = format!("h1-{dependency}-attempt-{attempt}");
    let state_root = receipt_path
        .parent()
        .ok_or_else(|| qualification("tool receipt path has no parent"))?
        .join(format!("command-runtime-{attempt}"));
    let process_store =
        peritus_process::ProcessStore::open(state_root.join("processes"), workspace)
            .map_err(|error| qualification(error.to_string()))?;
    let digest = sha256(scope.as_bytes());
    let mut run_bytes = [0_u8; 16];
    run_bytes.copy_from_slice(&digest.as_bytes()[..16]);
    let run_id = peritus_types::RunId::new(run_bytes)
        .map_err(|error| qualification(format!("construct qualification run: {error:?}")))?;
    let command_runtime =
        crate::CommandRuntime::open(state_root.join("router"), workspace, run_id, process_store)?;
    let mut tools = WorkspaceDeveloperTools::with_ownership(
        workspace.to_path_buf(),
        WorkspaceOwnership::capture(workspace),
        receipt_path.clone(),
        scope,
        Duration::from_secs(30),
        command_runtime,
    );
    require_success(
        &tools
            .execute(&call(
                &format!("h1-{dependency}-{attempt}-list"),
                "workspace_list",
                &object([("depth", Value::from(1_u64)), ("path", Value::String(String::new()))]),
            )?)
            .map_err(|error| qualification(error.to_string()))?,
        "ground tool workspace",
    )?;
    require_success(
        &tools
            .execute(&call(
                &format!("h1-{dependency}-{attempt}-read"),
                "workspace_read",
                &object([
                    ("end_line", Value::from(4_u64)),
                    ("path", Value::String("peritus-qualification.marker".to_owned())),
                    ("start_line", Value::from(1_u64)),
                ]),
            )?)
            .map_err(|error| qualification(error.to_string()))?,
        "read tool qualification marker",
    )?;
    let result = tools
        .execute(&call(
            &format!("h1-{dependency}-{attempt}-run"),
            "run_command",
            &object([
                (
                    "args",
                    Value::Array(vec![
                        Value::String("qualify-dependency-child".to_owned()),
                        Value::String(dependency.to_owned()),
                    ]),
                ),
                ("cwd", Value::String(".".to_owned())),
                ("program", Value::String(executable.to_owned())),
                ("timeout_seconds", Value::from(10_u64)),
            ]),
        )?)
        .map_err(|error| qualification(error.to_string()))?;
    let value: Value = serde_json::from_slice(result.output.canonical_bytes())
        .map_err(|error| qualification(error.to_string()))?;
    let success = value.get("success").and_then(Value::as_bool);
    let timed_out = value.get("timed_out").and_then(Value::as_bool);
    let exit_code =
        value.get("exit_code").and_then(Value::as_i64).and_then(|code| i32::try_from(code).ok());
    let stdout = value.get("stdout").and_then(Value::as_str);
    let stderr = value.get("stderr").and_then(Value::as_str);
    let disposition = value.get("disposition").and_then(Value::as_str);
    let os_exit =
        value.get("tool_result").and_then(|result| result.get("os_exit")).and_then(Value::as_str);
    let failure_code =
        value.get("failure").and_then(|failure| failure.get("code")).and_then(Value::as_str);
    if success != Some(false)
        || timed_out != Some(false)
        || exit_code != Some(17)
        || !result.is_error
        || stdout.is_none_or(|value| !value.contains("dependency-child"))
        || stderr != Some("")
    {
        return Err(qualification(format!(
            "bounded product tool did not retain the expected child death: success={success:?} timed_out={timed_out:?} exit_code={exit_code:?} disposition={disposition:?} os_exit={os_exit:?} failure_code={failure_code:?} is_error={} stdout_contains_marker={} stderr_empty={}",
            result.is_error,
            stdout.is_some_and(|value| value.contains("dependency-child")),
            stderr == Some(""),
        )));
    }
    let receipt_bytes =
        fs::metadata(receipt_path).map_err(|error| qualification(error.to_string()))?.len();
    if receipt_bytes == 0 {
        return Err(qualification("product tool effect receipt is empty"));
    }
    Ok(ToolProcessFailureObservation {
        exit_code: 17,
        stdout_sha256: sha256(stdout.unwrap_or_default().as_bytes()),
        receipt_bytes,
    })
}

fn call(id: &str, name: &str, arguments: &Value) -> Result<CompletedToolCall, ProductRunnerError> {
    let encoded =
        serde_json::to_string(&arguments).map_err(|error| qualification(error.to_string()))?;
    CompletedToolCall::new(
        ToolCallId::new(id.to_owned()).map_err(|_| qualification("tool call ID is invalid"))?,
        ToolName::new(name.to_owned()).map_err(|_| qualification("tool name is invalid"))?,
        CanonicalJson::parse(&encoded, JsonBounds::value(ProtocolLimits::PRODUCTION))
            .map_err(|error| qualification(error.to_string()))?,
    )
    .map_err(|error| qualification(error.to_string()))
}

fn object<const N: usize>(entries: [(&str, Value); N]) -> Value {
    Value::Object(
        entries.into_iter().map(|(name, value)| (name.to_owned(), value)).collect::<Map<_, _>>(),
    )
}

fn require_success(
    observation: &peritus_agent::DeveloperToolObservation,
    operation: &'static str,
) -> Result<(), ProductRunnerError> {
    if observation.is_error {
        return Err(ProductRunnerError::new(
            ProductRunnerErrorKind::Apply,
            operation,
            "qualification grounding tool returned an error",
        ));
    }
    Ok(())
}

fn sha256(bytes: &[u8]) -> Sha256Digest {
    let digest = Sha256::digest(bytes);
    let mut output = [0_u8; 32];
    output.copy_from_slice(&digest);
    Sha256Digest::new(output)
}

fn qualification(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Apply,
        "qualify product tool process failure",
        detail,
    )
}

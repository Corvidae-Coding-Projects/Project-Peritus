//! Structured command execution through the shared C4 router and C2 process owner.

use std::{collections::BTreeSet, path::PathBuf, time::Duration};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::WorkspaceDeveloperTools;
use crate::developer_tools::{
    command_budget::{CommandAllowance, CommandDeadlineSource},
    command_runtime::{CommandExecutionMode, StartCommand},
    effect::reject_destructive_command,
    path::{checked, tool},
    resources::CommandResources,
    wire::{bounded_u64, required_string, string},
};

const MAX_COMMAND_TIMEOUT_SECONDS: u64 = u64::MAX / 1_000;
const DEFAULT_TERMINAL_ROWS: u64 = 24;
const DEFAULT_TERMINAL_COLUMNS: u64 = 80;

struct ParsedCommand {
    program: String,
    arguments: Vec<String>,
    cwd: PathBuf,
    timeout: Option<Duration>,
    allowance: CommandAllowance,
    mode: CommandExecutionMode,
}

impl WorkspaceDeveloperTools {
    pub(super) fn command_has_effect(&self, arguments: &Value) -> Result<bool, DeveloperLoopError> {
        self.parse_command(arguments).map(|command| command.allowance.starts_execution())
    }

    pub(super) fn run_command(
        &mut self,
        arguments: &Value,
        call_id: &str,
    ) -> Result<Value, DeveloperLoopError> {
        let command = self.parse_command(arguments)?;
        if !command.allowance.starts_execution() {
            return Ok(command.allowance.exhausted_result());
        }
        let unowned_before = command
            .mode
            .is_mutation()
            .then(|| self.ownership.unowned_files(&self.root))
            .unwrap_or_default();
        let (environment, resource_evidence) =
            CommandResources::observe().select(&command.program, &command.arguments).into_parts();
        let request = StartCommand {
            program: &command.program,
            arguments: &command.arguments,
            cwd: &command.cwd,
            timeout: command.timeout,
            interactive: false,
            rows: u16::try_from(DEFAULT_TERMINAL_ROWS).expect("bounded terminal rows"),
            columns: u16::try_from(DEFAULT_TERMINAL_COLUMNS).expect("bounded terminal columns"),
            idempotency_key: call_id,
            environment,
        };
        let result =
            self.command_runtime()?.run_selected(request, command.mode, resource_evidence);
        self.finish_run_command(arguments, result, unowned_before, &command)
    }

    pub(super) async fn run_command_async(
        &mut self,
        arguments: &Value,
        call_id: &str,
    ) -> Result<Value, DeveloperLoopError> {
        let command = self.parse_command(arguments)?;
        if !command.allowance.starts_execution() {
            return Ok(command.allowance.exhausted_result());
        }
        let unowned_before = command
            .mode
            .is_mutation()
            .then(|| self.ownership.unowned_files(&self.root))
            .unwrap_or_default();
        let runtime = self.command_runtime()?.clone();
        let (environment, resource_evidence) =
            CommandResources::observe().select(&command.program, &command.arguments).into_parts();
        let request = StartCommand {
                program: &command.program,
                arguments: &command.arguments,
                cwd: &command.cwd,
                timeout: command.timeout,
                interactive: false,
                rows: u16::try_from(DEFAULT_TERMINAL_ROWS).expect("bounded terminal rows"),
                columns: u16::try_from(DEFAULT_TERMINAL_COLUMNS)
                    .expect("bounded terminal columns"),
                idempotency_key: call_id,
                environment,
            };
        let result = runtime.run_selected_async(request, command.mode, resource_evidence).await;
        self.finish_run_command(arguments, result, unowned_before, &command)
    }

    fn finish_run_command(
        &mut self,
        arguments: &Value,
        result: Result<Value, DeveloperLoopError>,
        unowned_before: BTreeSet<PathBuf>,
        command: &ParsedCommand,
    ) -> Result<Value, DeveloperLoopError> {
        match result {
            Ok(result) => {
                if matches!(result.get("state").and_then(Value::as_str), Some("running" | "starting")) {
                    // The original process can still produce files after this observation fails.
                    // Retain its admission preimage and purpose until same-handle settlement.
                    self.active_commands.started_named(
                        "run_command",
                        arguments,
                        &result,
                        unowned_before,
                        command.mode,
                    )?;
                } else if command.mode.is_mutation() {
                    self.ownership.record_command_creations(&self.root, &unowned_before);
                }
                annotate_result(self, result, command)
            }
            Err(error) => {
                if command.mode.is_mutation() {
                    self.ownership.record_command_creations(&self.root, &unowned_before);
                }
                Err(error)
            }
        }
    }

    pub(super) fn start_command(
        &mut self,
        arguments: &Value,
        call_id: &str,
    ) -> Result<Value, DeveloperLoopError> {
        let command = self.parse_command(arguments)?;
        if !command.allowance.starts_execution() {
            return Ok(command.allowance.exhausted_result());
        }
        let interactive = arguments.get("interactive").and_then(Value::as_bool).unwrap_or(true);
        let rows = bounded_u64(arguments, "rows", DEFAULT_TERMINAL_ROWS, 1, u16::MAX.into());
        let columns =
            bounded_u64(arguments, "columns", DEFAULT_TERMINAL_COLUMNS, 1, u16::MAX.into());
        let unowned_before = command
            .mode
            .is_mutation()
            .then(|| self.ownership.unowned_files(&self.root))
            .unwrap_or_default();
        let (environment, resource_evidence) =
            CommandResources::observe().select(&command.program, &command.arguments).into_parts();
        let request = StartCommand {
            program: &command.program,
            arguments: &command.arguments,
            cwd: &command.cwd,
            timeout: command.timeout,
            interactive,
            rows: u16::try_from(rows).expect("bounded terminal rows"),
            columns: u16::try_from(columns).expect("bounded terminal columns"),
            idempotency_key: call_id,
            environment,
        };
        let result = self.command_runtime()?.start_selected(
            request,
            command.mode,
            resource_evidence,
        )?;
        let result = annotate_result(self, result, &command)?;
        self.active_commands.started(arguments, &result, unowned_before, command.mode)?;
        Ok(result)
    }

    pub(super) async fn start_command_async(
        &mut self,
        arguments: &Value,
        call_id: &str,
    ) -> Result<Value, DeveloperLoopError> {
        let command = self.parse_command(arguments)?;
        if !command.allowance.starts_execution() {
            return Ok(command.allowance.exhausted_result());
        }
        let interactive = arguments.get("interactive").and_then(Value::as_bool).unwrap_or(true);
        let rows = bounded_u64(arguments, "rows", DEFAULT_TERMINAL_ROWS, 1, u16::MAX.into());
        let columns =
            bounded_u64(arguments, "columns", DEFAULT_TERMINAL_COLUMNS, 1, u16::MAX.into());
        let unowned_before = command
            .mode
            .is_mutation()
            .then(|| self.ownership.unowned_files(&self.root))
            .unwrap_or_default();
        let runtime = self.command_runtime()?.clone();
        let (environment, resource_evidence) =
            CommandResources::observe().select(&command.program, &command.arguments).into_parts();
        let request = StartCommand {
                program: &command.program,
                arguments: &command.arguments,
                cwd: &command.cwd,
                timeout: command.timeout,
                interactive,
                rows: u16::try_from(rows).expect("bounded terminal rows"),
                columns: u16::try_from(columns).expect("bounded terminal columns"),
                idempotency_key: call_id,
                environment,
            };
        let result = runtime
            .start_selected_async(request, command.mode, resource_evidence)
            .await?;
        let result = annotate_result(self, result, &command)?;
        self.active_commands.started(arguments, &result, unowned_before, command.mode)?;
        Ok(result)
    }

    pub(super) fn poll_command(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        self.command_runtime()?.poll(required_string(arguments, "handle")?)
    }

    pub(super) async fn poll_command_async(
        &self,
        arguments: &Value,
    ) -> Result<Value, DeveloperLoopError> {
        let handle = required_string(arguments, "handle")?.to_owned();
        let runtime = self.command_runtime()?.clone();
        runtime.poll_async(handle).await
    }

    pub(super) fn write_command_stdin(
        &self,
        arguments: &Value,
    ) -> Result<Value, DeveloperLoopError> {
        self.command_runtime()?.stdin(
            required_string(arguments, "handle")?,
            required_string(arguments, "text")?.as_bytes().to_vec(),
        )
    }

    pub(super) async fn write_command_stdin_async(
        &self,
        arguments: &Value,
    ) -> Result<Value, DeveloperLoopError> {
        let handle = required_string(arguments, "handle")?.to_owned();
        let bytes = required_string(arguments, "text")?.as_bytes().to_vec();
        let runtime = self.command_runtime()?.clone();
        runtime.stdin_async(handle, bytes).await
    }

    pub(super) fn resize_command(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        let rows = bounded_u64(arguments, "rows", 0, 0, u16::MAX.into());
        let columns = bounded_u64(arguments, "columns", 0, 0, u16::MAX.into());
        self.command_runtime()?.resize(
            required_string(arguments, "handle")?,
            u16::try_from(rows).expect("bounded terminal rows"),
            u16::try_from(columns).expect("bounded terminal columns"),
        )
    }

    pub(super) async fn resize_command_async(
        &self,
        arguments: &Value,
    ) -> Result<Value, DeveloperLoopError> {
        let rows = bounded_u64(arguments, "rows", 0, 0, u16::MAX.into());
        let columns = bounded_u64(arguments, "columns", 0, 0, u16::MAX.into());
        let handle = required_string(arguments, "handle")?.to_owned();
        let runtime = self.command_runtime()?.clone();
        runtime
            .resize_async(
                handle,
                u16::try_from(rows).expect("bounded terminal rows"),
                u16::try_from(columns).expect("bounded terminal columns"),
            )
            .await
    }

    pub(super) fn signal_command(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        self.command_runtime()?.signal(
            required_string(arguments, "handle")?,
            required_string(arguments, "signal")?.to_owned(),
        )
    }

    pub(super) async fn signal_command_async(
        &self,
        arguments: &Value,
    ) -> Result<Value, DeveloperLoopError> {
        let handle = required_string(arguments, "handle")?.to_owned();
        let signal = required_string(arguments, "signal")?.to_owned();
        let runtime = self.command_runtime()?.clone();
        runtime.signal_async(handle, signal).await
    }

    pub(super) fn cancel_command(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        self.command_runtime()?.cancel(required_string(arguments, "handle")?)
    }

    pub(super) async fn cancel_command_async(
        &self,
        arguments: &Value,
    ) -> Result<Value, DeveloperLoopError> {
        let handle = required_string(arguments, "handle")?.to_owned();
        let runtime = self.command_runtime()?.clone();
        runtime.cancel_async(handle).await
    }

    pub(super) fn recover_command(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        self.command_runtime()?.recover(required_string(arguments, "handle")?)
    }

    pub(super) async fn recover_command_async(
        &self,
        arguments: &Value,
    ) -> Result<Value, DeveloperLoopError> {
        let handle = required_string(arguments, "handle")?.to_owned();
        let runtime = self.command_runtime()?.clone();
        runtime.recover_async(handle).await
    }

    fn command_runtime(&self) -> Result<&crate::CommandRuntime, DeveloperLoopError> {
        self.command_runtime.as_ref().ok_or_else(|| tool("process tools have no command runtime"))
    }

    fn parse_command(&self, arguments: &Value) -> Result<ParsedCommand, DeveloperLoopError> {
        let program = required_string(arguments, "program")?;
        if program.is_empty() || program.contains(['\0', '\n', '\r']) {
            return Err(tool("command program is invalid"));
        }
        let args = arguments
            .get("args")
            .and_then(Value::as_array)
            .ok_or_else(|| tool("command args must be an array"))?
            .iter()
            .map(|value| {
                value.as_str().map(str::to_owned).ok_or_else(|| tool("command arg is not text"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        reject_destructive_command(program, &args)?;
        let cwd = match string(arguments, "cwd") {
            Some(value) if !value.is_empty() => checked(&self.root, value, false).map_err(|error| {
                tool(format!(
                    "command cwd must be a normal workspace-relative directory; omit cwd for the workspace root: {error}"
                ))
            })?,
            _ => self.root.clone(),
        };
        let requested_timeout_seconds = match arguments.get("timeout_seconds") {
            None => None,
            Some(value) => Some(value
                .as_u64()
                .filter(|seconds| (1..=MAX_COMMAND_TIMEOUT_SECONDS).contains(seconds))
                .ok_or_else(|| {
                    tool("timeout_seconds must be a positive integer representable in milliseconds")
                })?),
        };
        let mode = CommandExecutionMode::from_purpose(required_string(arguments, "purpose")?)
            .ok_or_else(|| tool("command purpose does not select a declared execution mode"))?;
        let allowance = self
            .command_budget
            .as_ref()
            .ok_or_else(|| tool("writable tools have no command budget"))?
            .allowance(requested_timeout_seconds);
        Ok(ParsedCommand {
            program: program.to_owned(),
            arguments: args,
            cwd,
            timeout: allowance.timeout,
            allowance,
            mode,
        })
    }
}

fn annotate_result(
    tools: &WorkspaceDeveloperTools,
    mut value: Value,
    command: &ParsedCommand,
) -> Result<Value, DeveloperLoopError> {
    let result =
        value.as_object_mut().ok_or_else(|| tool("command runtime returned non-object"))?;
    let timed_out = result.get("timed_out").and_then(Value::as_bool) == Some(true);
    let recovery_hint = timed_out.then(|| match command.allowance.deadline_source {
        CommandDeadlineSource::Product => {
            "The command reached the caller-selected product deadline. Active execution stopped at that deadline; exact command evidence and unfinished product obligations remain recoverable for an authorized resume."
        }
        CommandDeadlineSource::RequestedCommand => {
            "The command reached its requested positive timeout. Its terminal result is retained; retry with a longer positive timeout or a resumable strategy when the observed progress supports it."
        }
        CommandDeadlineSource::Untimed => {
            "The command runtime reported a timeout without an imposed command or product deadline. Reconcile the retained terminal result before deciding whether to retry."
        }
    });
    result.insert(
        "requested_timeout_seconds".to_owned(),
        command.allowance.requested_seconds.map_or(Value::Null, Value::from),
    );
    result.insert(
        "timeout_seconds".to_owned(),
        command.timeout.map_or(Value::Null, |timeout| Value::from(timeout.as_secs())),
    );
    result.insert(
        "timeout_millis".to_owned(),
        command.allowance.timeout_millis().map_or(Value::Null, Value::from),
    );
    result.insert(
        "deadline_source".to_owned(),
        Value::String(command.allowance.deadline_source.label().to_owned()),
    );
    result.insert(
        "deadline_limited".to_owned(),
        Value::Bool(command.allowance.deadline_limited()),
    );
    let remaining = tools
        .command_budget
        .as_ref()
        .ok_or_else(|| tool("writable tools have no command budget"))?
        .remaining();
    result.insert(
        "remaining_product_seconds".to_owned(),
        remaining.map_or(Value::Null, |remaining| Value::from(remaining.as_secs())),
    );
    result.insert(
        "remaining_product_millis".to_owned(),
        remaining
            .and_then(|remaining| u64::try_from(remaining.as_millis()).ok())
            .map_or(Value::Null, Value::from),
    );
    result.insert(
        "recovery_hint".to_owned(),
        recovery_hint.map_or(Value::Null, |hint| Value::String(hint.to_owned())),
    );
    Ok(value)
}

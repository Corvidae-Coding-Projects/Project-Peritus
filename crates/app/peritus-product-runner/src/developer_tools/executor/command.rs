//! Structured command execution through the shared C4 router and C2 process owner.

use std::{path::PathBuf, time::Duration};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::WorkspaceDeveloperTools;
use crate::developer_tools::{
    command_runtime::StartCommand,
    effect::reject_destructive_command,
    path::{checked, tool},
    wire::{bounded_u64, required_string, string},
};

const DEFAULT_COMMAND_TIMEOUT_SECONDS: u64 = 120;
const MAX_COMMAND_TIMEOUT_SECONDS: u64 = 600;
const DEFAULT_TERMINAL_ROWS: u64 = 24;
const DEFAULT_TERMINAL_COLUMNS: u64 = 80;

struct ParsedCommand {
    program: String,
    arguments: Vec<String>,
    cwd: PathBuf,
    timeout: Duration,
    requested_timeout_seconds: u64,
    deadline_limited: bool,
    completion_reserve_seconds: u64,
}

impl WorkspaceDeveloperTools {
    pub(super) fn run_command(
        &mut self,
        arguments: &Value,
        call_id: &str,
    ) -> Result<Value, DeveloperLoopError> {
        let command = self.parse_command(arguments)?;
        if command.timeout.is_zero() {
            return self.exhausted_result(command.requested_timeout_seconds);
        }
        let unowned_before = self.ownership.unowned_files(&self.root);
        let result = self.command_runtime()?.run(StartCommand {
            program: &command.program,
            arguments: &command.arguments,
            cwd: &command.cwd,
            timeout: command.timeout,
            interactive: false,
            rows: u16::try_from(DEFAULT_TERMINAL_ROWS).expect("bounded terminal rows"),
            columns: u16::try_from(DEFAULT_TERMINAL_COLUMNS).expect("bounded terminal columns"),
            idempotency_key: call_id,
            environment: self.resources.environment_bindings(),
        });
        self.ownership.record_command_creations(&self.root, &unowned_before);
        annotate_result(self, result?, &command)
    }

    pub(super) fn start_command(
        &mut self,
        arguments: &Value,
        call_id: &str,
    ) -> Result<Value, DeveloperLoopError> {
        let command = self.parse_command(arguments)?;
        if command.timeout.is_zero() {
            return self.exhausted_result(command.requested_timeout_seconds);
        }
        let interactive = arguments.get("interactive").and_then(Value::as_bool).unwrap_or(true);
        let rows = bounded_u64(arguments, "rows", DEFAULT_TERMINAL_ROWS, 1, u16::MAX.into());
        let columns =
            bounded_u64(arguments, "columns", DEFAULT_TERMINAL_COLUMNS, 1, u16::MAX.into());
        let unowned_before = self.ownership.unowned_files(&self.root);
        let result = self.command_runtime()?.start(StartCommand {
            program: &command.program,
            arguments: &command.arguments,
            cwd: &command.cwd,
            timeout: command.timeout,
            interactive,
            rows: u16::try_from(rows).expect("bounded terminal rows"),
            columns: u16::try_from(columns).expect("bounded terminal columns"),
            idempotency_key: call_id,
            environment: self.resources.environment_bindings(),
        })?;
        let result = annotate_result(self, result, &command)?;
        self.active_commands.started(arguments, &result, unowned_before)?;
        Ok(result)
    }

    pub(super) fn poll_command(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        self.command_runtime()?.poll(required_string(arguments, "handle")?)
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

    pub(super) fn resize_command(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        let rows = bounded_u64(arguments, "rows", 0, 0, u16::MAX.into());
        let columns = bounded_u64(arguments, "columns", 0, 0, u16::MAX.into());
        self.command_runtime()?.resize(
            required_string(arguments, "handle")?,
            u16::try_from(rows).expect("bounded terminal rows"),
            u16::try_from(columns).expect("bounded terminal columns"),
        )
    }

    pub(super) fn signal_command(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        self.command_runtime()?.signal(
            required_string(arguments, "handle")?,
            required_string(arguments, "signal")?.to_owned(),
        )
    }

    pub(super) fn cancel_command(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        self.command_runtime()?.cancel(required_string(arguments, "handle")?)
    }

    pub(super) fn recover_command(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        self.command_runtime()?.recover(required_string(arguments, "handle")?)
    }

    fn command_runtime(&self) -> Result<&crate::CommandRuntime, DeveloperLoopError> {
        self.command_runtime.as_ref().ok_or_else(|| tool("writable tools have no command runtime"))
    }

    fn exhausted_result(&self, requested: u64) -> Result<Value, DeveloperLoopError> {
        Ok(self
            .command_budget
            .as_ref()
            .ok_or_else(|| tool("writable tools have no command budget"))?
            .allowance(requested)
            .exhausted_result())
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
        self.resources.authorize(program, &args)?;
        let cwd = match string(arguments, "cwd") {
            Some(value) if !value.is_empty() => checked(&self.root, value, false)?,
            _ => self.root.clone(),
        };
        let requested_timeout_seconds = bounded_u64(
            arguments,
            "timeout_seconds",
            DEFAULT_COMMAND_TIMEOUT_SECONDS,
            1,
            MAX_COMMAND_TIMEOUT_SECONDS,
        );
        let allowance = self
            .command_budget
            .as_ref()
            .ok_or_else(|| tool("writable tools have no command budget"))?
            .allowance(requested_timeout_seconds);
        Ok(ParsedCommand {
            program: program.to_owned(),
            arguments: args,
            cwd,
            timeout: Duration::from_secs(allowance.timeout_seconds),
            requested_timeout_seconds: allowance.requested_seconds,
            deadline_limited: allowance.deadline_limited,
            completion_reserve_seconds: allowance.completion_reserve_seconds,
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
    let recovery_hint = timed_out.then_some({
        if command.deadline_limited {
            "The command reached the live product-budget allowance. Preserve the completion reserve, use existing evidence, and deliver the best verified result now."
        } else {
            "Do not retry an equivalent command with a longer timeout or another bulk-transfer wrapper without new size or progress evidence. Choose a materially bounded or resumable strategy."
        }
    });
    result.insert(
        "requested_timeout_seconds".to_owned(),
        Value::from(command.requested_timeout_seconds),
    );
    result.insert("timeout_seconds".to_owned(), Value::from(command.timeout.as_secs()));
    result.insert("deadline_limited".to_owned(), Value::Bool(command.deadline_limited));
    result.insert(
        "remaining_product_seconds".to_owned(),
        Value::from(
            tools
                .command_budget
                .as_ref()
                .ok_or_else(|| tool("writable tools have no command budget"))?
                .remaining_seconds(),
        ),
    );
    result.insert(
        "completion_reserve_seconds".to_owned(),
        Value::from(command.completion_reserve_seconds),
    );
    result.insert(
        "recovery_hint".to_owned(),
        recovery_hint.map_or(Value::Null, |hint| Value::String(hint.to_owned())),
    );
    Ok(value)
}

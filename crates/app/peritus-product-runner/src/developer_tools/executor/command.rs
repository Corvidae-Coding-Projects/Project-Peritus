//! Structured command execution within the managed workspace and caller budget.

use std::{process::Command, time::Duration};

use peritus_agent::DeveloperLoopError;
use serde_json::Value;

use super::WorkspaceDeveloperTools;
use crate::developer_tools::{
    effect::reject_destructive_command,
    path::{checked, tool},
    process,
    wire::{bounded_u64, object, required_string, string},
};

const DEFAULT_COMMAND_TIMEOUT_SECONDS: u64 = 120;
const MAX_COMMAND_TIMEOUT_SECONDS: u64 = 600;

impl WorkspaceDeveloperTools {
    pub(super) fn run(&mut self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
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
        let current_dir = match string(arguments, "cwd") {
            Some(value) if !value.is_empty() => checked(&self.root, value, false)?,
            _ => self.root.clone(),
        };
        let mut command = Command::new(program);
        command.args(&args).current_dir(current_dir);
        self.resources.prepare(&mut command, program, &args)?;
        let requested_timeout_seconds = bounded_u64(
            arguments,
            "timeout_seconds",
            DEFAULT_COMMAND_TIMEOUT_SECONDS,
            1,
            MAX_COMMAND_TIMEOUT_SECONDS,
        );
        let budget = self
            .command_budget
            .as_ref()
            .ok_or_else(|| tool("writable tools have no command budget"))?;
        let allowance = budget.allowance(requested_timeout_seconds);
        if allowance.timeout_seconds == 0 {
            return Ok(allowance.exhausted_result());
        }
        let unowned_before = self.ownership.unowned_files(&self.root);
        let output = process::run(command, Duration::from_secs(allowance.timeout_seconds));
        self.ownership.record_command_creations(&self.root, &unowned_before);
        let output = output?;
        let remaining_product_seconds = budget.remaining_seconds();
        let recovery_hint = if output.timed_out {
            Some(if allowance.deadline_limited {
                "The command reached the live product-budget allowance. Do not start another long \
                 command: preserve the completion reserve, use existing evidence, and deliver the \
                 best verified result now."
            } else {
                "Do not retry an equivalent command with a longer timeout or another bulk-transfer \
                 wrapper without new size or progress evidence. Preserve the task deadline and \
                 choose a materially bounded or resumable strategy."
            })
        } else {
            None
        };
        Ok(object(vec![
            ("success", Value::Bool(output.status.success() && !output.timed_out)),
            ("exit_code", output.status.code().map_or(Value::Null, Value::from)),
            ("stdout", Value::String(output.stdout)),
            ("stderr", Value::String(output.stderr)),
            ("timed_out", Value::Bool(output.timed_out)),
            ("requested_timeout_seconds", Value::from(allowance.requested_seconds)),
            ("timeout_seconds", Value::from(allowance.timeout_seconds)),
            ("deadline_limited", Value::Bool(allowance.deadline_limited)),
            ("remaining_product_seconds", Value::from(remaining_product_seconds)),
            ("completion_reserve_seconds", Value::from(allowance.completion_reserve_seconds)),
            (
                "recovery_hint",
                recovery_hint.map_or(Value::Null, |value| Value::String(value.to_owned())),
            ),
        ]))
    }
}

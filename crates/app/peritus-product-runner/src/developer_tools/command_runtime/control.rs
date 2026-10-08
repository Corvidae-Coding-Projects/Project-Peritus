//! Non-destructive validation and dispatch for active command controls.

use peritus_agent::DeveloperLoopError;
use peritus_tool_protocol::{BoundedText, ToolControl};
use serde_json::Value;

use super::{CommandRuntime, Observation, ObservationTarget};
use crate::developer_tools::{argument_contract, path::tool, wire::object};

impl CommandRuntime {
    pub(in crate::developer_tools) fn stdin(
        &self,
        handle: &str,
        bytes: Vec<u8>,
    ) -> Result<Value, DeveloperLoopError> {
        if self.active_interactive(handle)? == Some(false) {
            return Err(tool("stdin is disabled for this process"));
        }
        if bytes.is_empty() {
            return Err(tool("stdin input is empty"));
        }
        let requested = bytes.len();
        let mut acknowledged = 0_usize;
        let mut latest = None;
        for chunk in bytes.chunks(argument_contract::STDIN_CONTROL_BYTES) {
            let control = ToolControl::stdin(
                chunk.to_vec(),
                u32::try_from(argument_contract::STDIN_CONTROL_BYTES)
                    .expect("stdin control bound is represented by u32"),
            )
            .map_err(|error| tool(error.to_string()))?;
            let result = match self.observe(handle, Observation::Control(control)) {
                Ok(result) => result,
                Err(error) if acknowledged > 0 => {
                    return Ok(stdin_interrupted(handle, requested, acknowledged, &error));
                }
                Err(error) => return Err(error),
            };
            let admitted = control_admitted(&result);
            if admitted {
                acknowledged = acknowledged.saturating_add(chunk.len());
            }
            let stop = !admitted
                || result.get("state").and_then(Value::as_str) != Some("running")
                || result.get("success").and_then(Value::as_bool) == Some(false);
            latest = Some(result);
            if stop {
                break;
            }
        }
        annotate_stdin(
            latest.ok_or_else(|| tool("stdin produced no control observation"))?,
            requested,
            acknowledged,
        )
    }

    pub(in crate::developer_tools) async fn stdin_async(
        &self,
        handle: String,
        bytes: Vec<u8>,
    ) -> Result<Value, DeveloperLoopError> {
        if self.active_interactive(&handle)? == Some(false) {
            return Err(tool("stdin is disabled for this process"));
        }
        if bytes.is_empty() {
            return Err(tool("stdin input is empty"));
        }
        let requested = bytes.len();
        let mut acknowledged = 0_usize;
        let mut latest = None;
        for chunk in bytes.chunks(argument_contract::STDIN_CONTROL_BYTES) {
            let control = ToolControl::stdin(
                chunk.to_vec(),
                u32::try_from(argument_contract::STDIN_CONTROL_BYTES)
                    .expect("stdin control bound is represented by u32"),
            )
            .map_err(|error| tool(error.to_string()))?;
            let result = match self
                .observe_async(handle.clone(), Observation::Control(control))
                .await
            {
                Ok(result) => result,
                Err(error) if acknowledged > 0 => {
                    return Ok(stdin_interrupted(&handle, requested, acknowledged, &error));
                }
                Err(error) => return Err(error),
            };
            let admitted = control_admitted(&result);
            if admitted {
                acknowledged = acknowledged.saturating_add(chunk.len());
            }
            let stop = !admitted
                || result.get("state").and_then(Value::as_str) != Some("running")
                || result.get("success").and_then(Value::as_bool) == Some(false);
            latest = Some(result);
            if stop {
                break;
            }
        }
        annotate_stdin(
            latest.ok_or_else(|| tool("stdin produced no control observation"))?,
            requested,
            acknowledged,
        )
    }

    pub(in crate::developer_tools) fn resize(
        &self,
        handle: &str,
        rows: u16,
        columns: u16,
    ) -> Result<Value, DeveloperLoopError> {
        let interactive = self.active_interactive(handle)?;
        if interactive == Some(false) {
            return Err(tool("terminal resize was not authorized by the checked execution plan"));
        }
        if rows == 0 && columns == 0 {
            return annotate_resize(self.observe(handle, Observation::Poll)?, rows, columns, false);
        }
        if rows == 0 || columns == 0 {
            return Err(tool("terminal rows and columns must both be zero or both be positive"));
        }
        if platform_denies_resize(interactive) {
            return Err(tool("terminal resize was not authorized by the checked execution plan"));
        }
        let control =
            ToolControl::resize(rows, columns).map_err(|error| tool(error.to_string()))?;
        let result = self.observe(handle, Observation::Control(control))?;
        let admitted = control_admitted(&result);
        annotate_resize(result, rows, columns, admitted)
    }

    pub(in crate::developer_tools) async fn resize_async(
        &self,
        handle: String,
        rows: u16,
        columns: u16,
    ) -> Result<Value, DeveloperLoopError> {
        let interactive = self.active_interactive(&handle)?;
        if interactive == Some(false) {
            return Err(tool("terminal resize was not authorized by the checked execution plan"));
        }
        if rows == 0 && columns == 0 {
            let result = self.observe_async(handle, Observation::Poll).await?;
            return annotate_resize(result, rows, columns, false);
        }
        if rows == 0 || columns == 0 {
            return Err(tool("terminal rows and columns must both be zero or both be positive"));
        }
        if platform_denies_resize(interactive) {
            return Err(tool("terminal resize was not authorized by the checked execution plan"));
        }
        let control =
            ToolControl::resize(rows, columns).map_err(|error| tool(error.to_string()))?;
        let result = self.observe_async(handle, Observation::Control(control)).await?;
        let admitted = control_admitted(&result);
        annotate_resize(result, rows, columns, admitted)
    }

    pub(in crate::developer_tools) fn signal(
        &self,
        handle: &str,
        signal: String,
    ) -> Result<Value, DeveloperLoopError> {
        let signal = BoundedText::new(signal).map_err(|error| tool(error.to_string()))?;
        self.observe(handle, Observation::Control(ToolControl::Signal(signal)))
    }

    pub(in crate::developer_tools) async fn signal_async(
        &self,
        handle: String,
        signal: String,
    ) -> Result<Value, DeveloperLoopError> {
        let signal = BoundedText::new(signal).map_err(|error| tool(error.to_string()))?;
        self.observe_async(handle, Observation::Control(ToolControl::Signal(signal))).await
    }

    pub(in crate::developer_tools) fn cancel(
        &self,
        handle: &str,
    ) -> Result<Value, DeveloperLoopError> {
        self.observe(handle, Observation::Cancel)
    }

    pub(in crate::developer_tools) async fn cancel_async(
        &self,
        handle: String,
    ) -> Result<Value, DeveloperLoopError> {
        let mode = self.projection_mode(&handle)?;
        let control = match self.observation_target(&handle)? {
            ObservationTarget::Active { control: Some(control), .. } => control,
            ObservationTarget::Terminal(_)
            | ObservationTarget::Recovered(_)
            | ObservationTarget::Starting { .. }
            | ObservationTarget::Active { control: None, .. } => {
                return self.observe_async(handle, Observation::Cancel).await;
            }
        };
        let mut admitted = false;
        if control.terminal_result().is_none() {
            match control.cancel(peritus_process::CancellationReason::User) {
                Ok(()) => admitted = true,
                Err(error) if error.recovery() == peritus_process::RecoveryClass::Terminal => {}
                Err(error) => return Err(tool(error.to_string())),
            }
        }
        let process_terminal = control.terminal_result().is_some();
        self.inner.cancellation_worker.enqueue_observer_reconciliation(
            self.clone(),
            handle.clone(),
            Some(control),
        );
        let value = super::result::cancellation_pending(&handle, admitted, process_terminal);
        Ok(mode.map_or(value.clone(), |mode| super::with_execution_mode(value, mode)))
    }

    pub(in crate::developer_tools) fn recover(
        &self,
        handle: &str,
    ) -> Result<Value, DeveloperLoopError> {
        self.observe(handle, Observation::Recover)
    }

    pub(in crate::developer_tools) async fn recover_async(
        &self,
        handle: String,
    ) -> Result<Value, DeveloperLoopError> {
        self.observe_async(handle, Observation::Recover).await
    }

    fn active_interactive(&self, handle: &str) -> Result<Option<bool>, DeveloperLoopError> {
        let state = self.inner.state.lock().map_err(|_| tool("command runtime is poisoned"))?;
        Ok(state.active.get(handle).map(|command| command.interactive))
    }
}

fn control_admitted(value: &Value) -> bool {
    value.get("control_admitted").and_then(Value::as_bool) == Some(true)
}

fn annotate_stdin(
    mut value: Value,
    requested: usize,
    acknowledged: usize,
) -> Result<Value, DeveloperLoopError> {
    let result = value
        .as_object_mut()
        .ok_or_else(|| tool("stdin control observation is not an object"))?;
    result.insert(
        "contract_version".to_owned(),
        Value::String(argument_contract::VERSION.to_owned()),
    );
    result.insert("stdin_acknowledged_bytes".to_owned(), Value::from(acknowledged));
    result.insert("stdin_complete".to_owned(), Value::Bool(acknowledged == requested));
    result.insert("stdin_next_byte".to_owned(), Value::from(acknowledged));
    result.insert("stdin_requested_bytes".to_owned(), Value::from(requested));
    Ok(value)
}

fn stdin_interrupted(
    handle: &str,
    requested: usize,
    acknowledged: usize,
    error: &DeveloperLoopError,
) -> Value {
    object(vec![
        ("contract_version", Value::String(argument_contract::VERSION.to_owned())),
        ("failure_detail", Value::String(error.to_string())),
        ("failure_kind", Value::String("temporary_backend".to_owned())),
        ("handle", Value::String(handle.to_owned())),
        ("owner_retained", Value::Null),
        ("reconciliation_pending", Value::Bool(true)),
        ("state", Value::String("running".to_owned())),
        ("stdin_acknowledged_bytes", Value::from(acknowledged)),
        ("stdin_complete", Value::Bool(false)),
        ("stdin_next_byte", Value::from(acknowledged)),
        ("stdin_requested_bytes", Value::from(requested)),
        ("success", Value::Bool(false)),
    ])
}

fn annotate_resize(
    mut value: Value,
    rows: u16,
    columns: u16,
    applied: bool,
) -> Result<Value, DeveloperLoopError> {
    let result = value
        .as_object_mut()
        .ok_or_else(|| tool("resize observation is not an object"))?;
    result.insert(
        "contract_version".to_owned(),
        Value::String(argument_contract::VERSION.to_owned()),
    );
    result.insert("resize_applied".to_owned(), Value::Bool(applied));
    result.insert("resize_columns".to_owned(), Value::from(columns));
    result.insert("resize_rows".to_owned(), Value::from(rows));
    Ok(value)
}

#[cfg(windows)]
fn platform_denies_resize(interactive: Option<bool>) -> bool {
    interactive == Some(true)
}

#[cfg(not(windows))]
const fn platform_denies_resize(_interactive: Option<bool>) -> bool {
    false
}

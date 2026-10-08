//! Non-destructive validation and dispatch for active command controls.

use peritus_agent::DeveloperLoopError;
use peritus_tool_protocol::{BoundedText, ToolControl};
use serde_json::Value;

use super::{CommandRuntime, Observation, ObservationTarget};
use crate::developer_tools::path::tool;

impl CommandRuntime {
    pub(in crate::developer_tools) fn stdin(
        &self,
        handle: &str,
        bytes: Vec<u8>,
    ) -> Result<Value, DeveloperLoopError> {
        if self.active_interactive(handle)? == Some(false) {
            return Err(tool("stdin is disabled for this process"));
        }
        let control = ToolControl::stdin(bytes, 65_536).map_err(|error| tool(error.to_string()))?;
        self.observe(handle, Observation::Control(control))
    }

    pub(in crate::developer_tools) async fn stdin_async(
        &self,
        handle: String,
        bytes: Vec<u8>,
    ) -> Result<Value, DeveloperLoopError> {
        if self.active_interactive(&handle)? == Some(false) {
            return Err(tool("stdin is disabled for this process"));
        }
        let control = ToolControl::stdin(bytes, 65_536).map_err(|error| tool(error.to_string()))?;
        self.observe_async(handle, Observation::Control(control)).await
    }

    pub(in crate::developer_tools) fn resize(
        &self,
        handle: &str,
        rows: u16,
        columns: u16,
    ) -> Result<Value, DeveloperLoopError> {
        let interactive = self.active_interactive(handle)?;
        if interactive == Some(false) || platform_denies_resize(interactive) {
            return Err(tool("terminal resize was not authorized by the checked execution plan"));
        }
        let control =
            ToolControl::resize(rows, columns).map_err(|error| tool(error.to_string()))?;
        self.observe(handle, Observation::Control(control))
    }

    pub(in crate::developer_tools) async fn resize_async(
        &self,
        handle: String,
        rows: u16,
        columns: u16,
    ) -> Result<Value, DeveloperLoopError> {
        let interactive = self.active_interactive(&handle)?;
        if interactive == Some(false) || platform_denies_resize(interactive) {
            return Err(tool("terminal resize was not authorized by the checked execution plan"));
        }
        let control =
            ToolControl::resize(rows, columns).map_err(|error| tool(error.to_string()))?;
        self.observe_async(handle, Observation::Control(control)).await
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

#[cfg(windows)]
fn platform_denies_resize(interactive: Option<bool>) -> bool {
    interactive == Some(true)
}

#[cfg(not(windows))]
const fn platform_denies_resize(_interactive: Option<bool>) -> bool {
    false
}

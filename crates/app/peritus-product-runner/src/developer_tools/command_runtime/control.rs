//! Non-destructive validation and dispatch for active command controls.

use peritus_agent::DeveloperLoopError;
use peritus_tool_protocol::{BoundedText, ToolControl};
use serde_json::Value;

use super::{CommandRuntime, Observation};
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

    pub(in crate::developer_tools) fn signal(
        &self,
        handle: &str,
        signal: String,
    ) -> Result<Value, DeveloperLoopError> {
        let signal = BoundedText::new(signal).map_err(|error| tool(error.to_string()))?;
        self.observe(handle, Observation::Control(ToolControl::Signal(signal)))
    }

    pub(in crate::developer_tools) fn cancel(
        &self,
        handle: &str,
    ) -> Result<Value, DeveloperLoopError> {
        self.observe(handle, Observation::Cancel)
    }

    pub(in crate::developer_tools) fn recover(
        &self,
        handle: &str,
    ) -> Result<Value, DeveloperLoopError> {
        self.observe(handle, Observation::Recover)
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

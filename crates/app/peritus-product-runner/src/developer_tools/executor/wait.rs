//! Passive waits retain the original C4/C2 handle and release every owner lock between polls.

use std::time::{Duration, Instant};

use peritus_agent::{DeveloperControlFlow, DeveloperLoopError};
use serde_json::{Value, json};

use super::WorkspaceDeveloperTools;
use crate::developer_tools::{path::tool, wire::required_string};

const CONTROL_INTERVAL: Duration = Duration::from_millis(100);

struct WaitControl {
    input_revision: u64,
    permissions: Option<crate::control::HostPermissions>,
    permission_revision: Option<u64>,
}

impl WorkspaceDeveloperTools {
    pub(super) fn wait_command(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        if tokio::runtime::Handle::try_current().is_ok_and(|handle| {
            matches!(handle.runtime_flavor(), tokio::runtime::RuntimeFlavor::MultiThread)
        }) {
            return tokio::task::block_in_place(|| self.wait_owned(arguments));
        }
        self.wait_owned(arguments)
    }

    fn wait_owned(&self, arguments: &Value) -> Result<Value, DeveloperLoopError> {
        let handle = required_string(arguments, "handle")?;
        let runtime =
            self.command_runtime.as_ref().ok_or_else(|| tool("command runtime unavailable"))?;
        let view = self.protection_view.as_deref();
        let revision = match view.and_then(crate::ConversationView::interaction) {
            Some(port) => port.input()?.revision,
            None => view.map_or(0, crate::ConversationView::revision),
        };
        let control = WaitControl {
            input_revision: revision,
            permissions: view.map(crate::ConversationView::effective_permissions),
            permission_revision: view
                .map(crate::ConversationView::permission_revision)
                .transpose()
                .map_err(tool)?
                .flatten(),
        };
        let limit = arguments["max_wait_ms"].as_u64().map(Duration::from_millis);
        let output_request = json!({"handle":handle,
            "stream":arguments["stream"].as_str().unwrap_or("stdout"),
            "offset":arguments["after_bytes"].as_u64().unwrap_or(0),"max_bytes":16384});
        let started = Instant::now();
        loop {
            if let Some(reason) = self.wait_interruption(&control)? {
                return Ok(yielded(handle, reason, started));
            }
            let mut observation = runtime.poll(handle)?;
            if observation["state"].as_str() != Some("running") {
                if let Some(reason) = self.wait_interruption(&control)? {
                    return Ok(yielded(handle, reason, started));
                }
                observation["wait"] = metadata("terminal_or_unknown", started);
                return Ok(observation);
            }
            if arguments["until"].as_str() == Some("output_or_terminal") {
                let output = runtime.read_command_output(&output_request)?;
                if output["returned_bytes"].as_u64().is_some_and(|bytes| bytes > 0) {
                    if let Some(reason) = self.wait_interruption(&control)? {
                        return Ok(yielded(handle, reason, started));
                    }
                    observation["output"] = output;
                    observation["wait"] = metadata("output", started);
                    return Ok(observation);
                }
            }
            if limit.is_some_and(|limit| started.elapsed() >= limit) {
                if let Some(reason) = self.wait_interruption(&control)? {
                    return Ok(yielded(handle, reason, started));
                }
                observation["wait"] = metadata("wait_limit", started);
                return Ok(observation);
            }
            let delay = limit.map_or(CONTROL_INTERVAL, |limit| {
                CONTROL_INTERVAL.min(limit.saturating_sub(started.elapsed()))
            });
            // The runtime poll/output read above have returned and released all owner locks.
            std::thread::sleep(delay);
        }
    }

    fn wait_interruption(
        &self,
        control: &WaitControl,
    ) -> Result<Option<&'static str>, DeveloperLoopError> {
        if self.inspection_cancellation.check().is_err() {
            return Ok(Some("cancelled"));
        }
        if self.permission_denial("command_wait").is_some() {
            return Ok(Some("permission_changed"));
        }
        let Some(view) = self.protection_view.as_deref() else { return Ok(None) };
        if Some(view.effective_permissions()) != control.permissions
            || view.permission_revision().map_err(tool)? != control.permission_revision
        {
            return Ok(Some("permission_changed"));
        }
        let control = match view.interaction() {
            Some(port) => port.tool_control(control.input_revision)?,
            None if view.revision() != control.input_revision => DeveloperControlFlow::Yield,
            None => DeveloperControlFlow::Continue,
        };
        Ok((control != DeveloperControlFlow::Continue).then_some("control_changed"))
    }
}

fn metadata(reason: &str, started: Instant) -> Value {
    json!({"reason":reason,"elapsed_ms":u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
        "redispatched":false})
}

fn yielded(handle: &str, reason: &str, started: Instant) -> Value {
    json!({"handle":handle,"state":"unobserved","success":true,"wait":metadata(reason, started),
        "recovery_hint":"Only the wait ended. Reconcile or control this original handle; never restart it from this observation."})
}

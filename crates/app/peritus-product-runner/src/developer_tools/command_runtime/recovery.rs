use super::{
    ArtifactStore, CancellationReason, CommandRuntime, DeveloperLoopError, NativeCommandOwner,
    Observation, RecoveredCommandOwner, RecoveryDisposition, ToolControl, Value, identity, result,
    retain_projection_with_owner, runtime_open, tool,
};
use peritus_types::{ActionId, EventId, ProcessId, RunId};
use std::path::Path;

impl CommandRuntime {
    /// Returns exact live process owners linked to command receipts.
    ///
    /// This read-only startup probe lets the daemon preserve only process trees whose durable
    /// receipt, process manifest, consumption claim, and native identity all agree.
    ///
    /// # Errors
    /// Returns an error when the receipt history or native owner evidence is invalid.
    pub fn receipt_linked_live_owners(
        receipt_path: &Path,
        source_run: RunId,
        processes: &peritus_process::ProcessStore,
    ) -> Result<Vec<(RunId, ActionId, ProcessId)>, crate::ProductRunnerError> {
        let owners = super::super::receipt::receipt_linked_native_owners(receipt_path)
            .map_err(|error| runtime_open(format!("inspect command owner receipts: {error}")))?;
        let mut exact = Vec::new();
        let mut probe = peritus_process::NativeProcessProbe::new();
        for owner in owners {
            if owner.source_run != source_run {
                continue;
            }
            let observed = processes
                .observe_exact(owner.execution_run, owner.action, owner.process, &mut probe)
                .map_err(|error| {
                    runtime_open(format!("observe exact native command owner: {error}"))
                })?;
            if observed.disposition() == RecoveryDisposition::LiveOwned {
                let identity = (owner.execution_run, owner.action, owner.process);
                if !exact.contains(&identity) {
                    exact.push(identity);
                }
            }
        }
        Ok(exact)
    }

    /// Reconciles exact native command owners retained by an existing effect receipt file.
    ///
    /// This only observes the process store and appends recovered terminal or inactive-owner
    /// receipt evidence. It never dispatches a provider call, starts a command, or terminates a
    /// live process.
    ///
    /// # Errors
    /// Returns an error when the receipt history or its exact native owner evidence is invalid.
    pub fn reconcile_effect_receipts(
        &self,
        receipt_path: &Path,
    ) -> Result<(), crate::ProductRunnerError> {
        let mut receipts = super::super::receipt::EffectReceiptLedger::new(
            receipt_path.to_path_buf(),
            "peritus-daemon-recovery".to_owned(),
        );
        let owners = receipts
            .uncertain_command_owners()
            .map_err(|error| runtime_open(format!("inspect command owner receipts: {error}")))?;
        for owner in owners {
            let recovered = self
                .recover_receipt_owner(owner)
                .map_err(|error| runtime_open(format!("recover native command owner: {error}")))?;
            receipts
                .reconcile_native_command_owner(owner, recovered.disposition, &recovered.value)
                .map_err(|error| {
                    runtime_open(format!("persist native command recovery: {error}"))
                })?;
        }
        Ok(())
    }

    pub(in crate::developer_tools) fn recover_receipt_owner(
        &self,
        owner: NativeCommandOwner,
    ) -> Result<RecoveredCommandOwner, DeveloperLoopError> {
        let handle = identity::action_hex(owner.action);
        if owner.source_run != self.inner.run_id {
            return Ok(RecoveredCommandOwner {
                disposition: RecoveryDisposition::Indeterminate,
                value: result::indeterminate(
                    &handle,
                    "the command receipt owner belongs to a different runtime run",
                ),
            });
        }
        let invocation = {
            let state = self.inner.state.lock().map_err(|_| tool("command runtime is poisoned"))?;
            state.active.get(&handle).and_then(|active| {
                let identity = active.plan.identity();
                (identity.run_id() == owner.execution_run
                    && identity.action_id() == owner.action
                    && identity.process_id() == owner.process)
                    .then_some(active.invocation)
            })
        };
        if invocation.is_some() {
            let value = self.observe(&handle, Observation::Recover)?;
            let disposition = match value.get("state").and_then(Value::as_str) {
                Some("running") => RecoveryDisposition::LiveOwned,
                Some("completed") => RecoveryDisposition::Terminal,
                _ => RecoveryDisposition::Indeterminate,
            };
            return Ok(RecoveredCommandOwner { disposition, value });
        }

        let mut probe = peritus_process::NativeProcessProbe::new();
        let exact = self
            .inner
            .process_store
            .observe_exact(owner.execution_run, owner.action, owner.process, &mut probe)
            .map_err(|error| tool(format!("observe exact native command owner: {error}")))?;
        let disposition = exact.disposition();
        let value = match disposition {
            RecoveryDisposition::Terminal => self.durable_terminal_value(owner, &handle)?,
            RecoveryDisposition::LiveOwned => result::active(&handle, &[]),
            RecoveryDisposition::AbsentUnobserved => {
                let mut value = result::indeterminate(
                    &handle,
                    "the exact native owner is inactive, but no terminal result was durably recorded; its outcome remains unknown",
                );
                if let Some(fields) = value.as_object_mut() {
                    fields.insert("owner_inactive".to_owned(), Value::Bool(true));
                }
                value
            }
            RecoveryDisposition::Indeterminate => result::indeterminate(
                &handle,
                "the exact native command owner could not be verified safely",
            ),
        };
        retain_projection_with_owner(&self.inner.state_root, &handle, value.clone(), owner);
        Ok(RecoveredCommandOwner { disposition, value })
    }

    pub(super) fn observe_recovered_owner(
        &self,
        owner: NativeCommandOwner,
        operation: Observation,
    ) -> Result<Value, DeveloperLoopError> {
        let handle = identity::action_hex(owner.action);
        if owner.source_run != self.inner.run_id {
            return Ok(result::indeterminate(
                &handle,
                "the command handle belongs to a different runtime run",
            ));
        }
        let mut probe = peritus_process::NativeProcessProbe::new();
        let observed = self
            .inner
            .process_store
            .observe_exact(owner.execution_run, owner.action, owner.process, &mut probe)
            .map_err(|error| tool(format!("observe exact native command owner: {error}")))?;
        let value = match observed.disposition() {
            RecoveryDisposition::Terminal => self.durable_terminal_value(owner, &handle),
            RecoveryDisposition::LiveOwned => {
                let control = self.inner.process_store.control_exact(
                    owner.execution_run,
                    owner.action,
                    owner.process,
                );
                match operation {
                    Observation::Poll | Observation::Recover => {}
                    Observation::Control(ToolControl::Cancel(_)) | Observation::Cancel
                        if control.is_none() =>
                    {
                        return self.cancel_recovered_owner(owner, &handle);
                    }
                    Observation::Control(request) => {
                        let control = control.ok_or_else(|| {
                            tool("exact live command owner has no reconnectable control")
                        })?;
                        apply_recovered_control(&control, request)?;
                    }
                    Observation::Cancel => {
                        let control = control.ok_or_else(|| {
                            tool("exact live command owner has no reconnectable control")
                        })?;
                        control
                            .cancel(peritus_process::CancellationReason::User)
                            .map_err(|error| tool(error.to_string()))?;
                    }
                }
                Ok(result::active(&handle, &[]))
            }
            RecoveryDisposition::AbsentUnobserved => {
                let mut value = result::indeterminate(
                    &handle,
                    "the exact native owner is inactive, but no terminal result was durably recorded; its outcome remains unknown",
                );
                if let Some(fields) = value.as_object_mut() {
                    fields.insert("owner_inactive".to_owned(), Value::Bool(true));
                }
                Ok(value)
            }
            RecoveryDisposition::Indeterminate => Ok(result::indeterminate(
                &handle,
                "the exact native command owner could not be verified safely",
            )),
        }?;
        retain_projection_with_owner(&self.inner.state_root, &handle, value.clone(), owner);
        Ok(value)
    }

    fn cancel_recovered_owner(
        &self,
        owner: NativeCommandOwner,
        handle: &str,
    ) -> Result<Value, DeveloperLoopError> {
        let mut probe = peritus_process::NativeProcessProbe::new();
        let outcome = self
            .inner
            .process_store
            .reconcile_exact(owner.execution_run, owner.action, owner.process, &mut probe)
            .map_err(|error| tool(format!("cancel exact native command owner: {error}")))?;
        let value = match outcome.disposition() {
            RecoveryDisposition::Terminal => self.durable_terminal_value(owner, handle),
            RecoveryDisposition::LiveOwned => Ok(result::active(handle, &[])),
            RecoveryDisposition::AbsentUnobserved => {
                let mut value = result::indeterminate(
                    handle,
                    "the exact native command owner stopped, but no terminal result was durably recorded; its outcome remains unknown",
                );
                if let Some(fields) = value.as_object_mut() {
                    fields.insert("owner_inactive".to_owned(), Value::Bool(true));
                }
                Ok(value)
            }
            RecoveryDisposition::Indeterminate => Ok(result::indeterminate(
                handle,
                "the exact native command owner could not be cancelled safely",
            )),
        }?;
        retain_projection_with_owner(&self.inner.state_root, handle, value.clone(), owner);
        Ok(value)
    }

    fn durable_terminal_value(
        &self,
        owner: NativeCommandOwner,
        handle: &str,
    ) -> Result<Value, DeveloperLoopError> {
        let artifacts = ArtifactStore::open(self.inner.artifacts.clone())
            .map_err(|error| tool(format!("reopen command artifact store: {error}")))?;
        let terminal = self
            .inner
            .process_store
            .terminal_result(owner.process)
            .map_err(|error| tool(format!("read exact terminal command result: {error}")))?;
        let terminal = if terminal.artifact_publication_complete() {
            terminal
        } else {
            self.inner
                .process_store
                .retry_artifact_publication(
                    owner.process,
                    &artifacts,
                    recovered_artifact_event(owner)?,
                )
                .map_err(|error| tool(format!("recover exact command output artifacts: {error}")))?
        };
        result::terminal_from_durable_owner(handle, &terminal, &self.inner.artifacts).map_err(tool)
    }
}

fn apply_recovered_control(
    control: &peritus_process::ProcessControl,
    request: ToolControl,
) -> Result<(), DeveloperLoopError> {
    match request {
        ToolControl::Poll => Ok(()),
        ToolControl::Stdin(bytes) => {
            control.write_stdin(bytes).map_err(|error| tool(error.to_string()))
        }
        ToolControl::Resize { rows, columns } => {
            let size = peritus_process::TerminalSize::new(rows, columns, 0, 0)
                .map_err(|error| tool(error.to_string()))?;
            control.resize(size).map_err(|error| tool(error.to_string()))
        }
        ToolControl::Signal(name) => {
            let signal = match name.as_str() {
                "INT" | "SIGINT" | "interrupt" => peritus_process::ProcessSignal::Interrupt,
                "TERM" | "SIGTERM" | "terminate" => peritus_process::ProcessSignal::Terminate,
                _ => return Err(tool("unsupported recovered command signal")),
            };
            control.signal(signal).map_err(|error| tool(error.to_string()))
        }
        ToolControl::Cancel(reason) => {
            let reason = match reason {
                CancellationReason::Requested => peritus_process::CancellationReason::User,
                CancellationReason::Deadline => peritus_process::CancellationReason::Deadline,
                CancellationReason::Shutdown => {
                    peritus_process::CancellationReason::SupervisorShutdown
                }
                CancellationReason::Recovery => peritus_process::CancellationReason::BackendFailure,
            };
            control.cancel(reason).map_err(|error| tool(error.to_string()))
        }
    }
}

fn recovered_artifact_event(owner: NativeCommandOwner) -> Result<EventId, DeveloperLoopError> {
    use sha2::{Digest as _, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(b"peritus-recovered-command-artifact-v1");
    hasher.update(owner.source_run.as_bytes());
    hasher.update(owner.execution_run.as_bytes());
    hasher.update(owner.action.as_bytes());
    hasher.update(owner.process.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    if bytes.iter().all(|byte| *byte == 0) {
        bytes[15] = 1;
    }
    EventId::new(bytes).map_err(|error| tool(format!("derive recovered artifact event: {error:?}")))
}

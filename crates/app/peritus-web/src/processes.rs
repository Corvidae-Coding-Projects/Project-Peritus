//! Browser command observers reuse the C4/C2 owner, durable identity, and full output spools.

use crate::{
    error::{Result, problem, uncertain},
    state::{App, hex},
};
use peritus_process::{OutputStream, ProcessControl, ProcessStore, TerminalSize};
use peritus_product_runner::{
    CommandRuntime, PreviewCommand, PreviewLaunch, PreviewObservation, PreviewOwner,
    PreviewProcessState,
};
use peritus_types::{ActionId, ProcessId, RunId};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedCommand {
    root: PathBuf,
    source: [u8; 16],
    execution: [u8; 16],
    action: [u8; 16],
    process: [u8; 16],
    interactive: bool,
}
impl SavedCommand {
    fn owner(&self) -> Result<PreviewOwner> {
        let failure = |error| problem(format!("Invalid retained command identity: {error:?}"));
        Ok(PreviewOwner::new(
            RunId::new(self.source).map_err(failure)?,
            RunId::new(self.execution).map_err(failure)?,
            ActionId::new(self.action).map_err(failure)?,
            ProcessId::new(self.process).map_err(failure)?,
        ))
    }
}

pub struct ManagedCommand {
    runtime: CommandRuntime,
    launch: PreviewLaunch,
    owner: PreviewOwner,
    control: Option<ProcessControl>,
    // Declared last so the runtime and process owners are dropped before temporary state.
    _temporary: Option<tempfile::TempDir>,
}

impl ManagedCommand {
    pub(crate) fn start(
        app: &App,
        key: &str,
        root: &Path,
        program: String,
        arguments: Vec<String>,
        interactive: bool,
    ) -> Result<Arc<Self>> {
        if app.snapshot()?.commands.contains_key(key) {
            return Err(uncertain(
                "This command has an existing owner; inspect its original outcome instead of repeating it",
            ));
        }
        let source = scope(key)?;
        let runtime = open_runtime(app, root, source)?;
        let command = PreviewCommand::new_optional(
            program,
            arguments,
            root.to_path_buf(),
            None,
            interactive,
            30,
            100,
            hex(source.as_bytes()),
            vec![
                ("TERM".into(), "xterm-256color".into()),
                ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ],
        )
        .map_err(problem)?;
        let mut owner = None;
        let result = runtime.launch_preview_registered(&command, &mut |binding| {
            let saved = SavedCommand {
                root: root.to_path_buf(),
                source: *source.as_bytes(),
                execution: *binding.execution_run().as_bytes(),
                action: *binding.action().as_bytes(),
                process: *binding.process().as_bytes(),
                interactive,
            };
            app.update(|state| {
                if state.commands.contains_key(key) {
                    return Err(problem("Command owner already exists"));
                }
                state.commands.insert(key.to_owned(), saved);
                Ok(())
            })
            .map_err(|error| error.to_string())?;
            owner = Some(binding);
            Ok(())
        });
        let launch = result
            .map_err(|error| if owner.is_some() { uncertain(error) } else { problem(error) })?;
        let owner = owner
            .ok_or_else(|| uncertain("Command launched without an observable retained binding"))?;
        let control = if interactive {
            Some(runtime.preview_terminal(&launch).map_err(uncertain)?.control())
        } else {
            None
        };
        let managed = Arc::new(Self { runtime, launch, owner, control, _temporary: None });
        app.processes.lock().map_err(problem)?.insert(key.to_owned(), Arc::clone(&managed));
        Ok(managed)
    }

    pub(crate) fn temporary(
        root: &Path,
        program: String,
        arguments: Vec<String>,
    ) -> Result<Arc<Self>> {
        let temporary = tempfile::tempdir()?;
        let source = scope(&crate::state::id()?)?;
        let processes =
            ProcessStore::open(temporary.path().join("processes"), root).map_err(problem)?;
        let runtime =
            CommandRuntime::open(temporary.path().join("runtime"), root, source, processes)
                .map_err(problem)?;
        let command = PreviewCommand::new_optional(
            program,
            arguments,
            root.to_path_buf(),
            None,
            false,
            30,
            100,
            hex(source.as_bytes()),
            vec![
                ("GIT_TERMINAL_PROMPT".into(), "0".into()),
                ("GIT_OPTIONAL_LOCKS".into(), "0".into()),
            ],
        )
        .map_err(problem)?;
        let mut owner = None;
        let launch = runtime
            .launch_preview_registered(&command, &mut |binding| {
                owner = Some(binding);
                Ok(())
            })
            .map_err(problem)?;
        let owner = owner.ok_or_else(|| problem("Temporary command owner missing"))?;
        Ok(Arc::new(Self { runtime, launch, owner, control: None, _temporary: Some(temporary) }))
    }

    pub(crate) fn get(app: &App, key: &str) -> Result<Arc<Self>> {
        let retained = app.processes.lock().map_err(problem)?.get(key).cloned();
        if let Some(command) = retained {
            return Ok(command);
        }
        let saved = app
            .snapshot()?
            .commands
            .get(key)
            .cloned()
            .ok_or_else(|| problem("Saved command not found"))?;
        let owner = saved.owner()?;
        let runtime = open_runtime(app, &saved.root, owner.source_run())?;
        let launch = runtime.reconnect_preview(owner).map_err(uncertain)?;
        // Reopening never fabricates a live stdin handle. Retained output and exact cancellation
        // remain available, while a new terminal is required for interactive input after restart.
        Ok(Arc::new(Self { runtime, launch, owner, control: None, _temporary: None }))
    }

    pub(crate) fn observe(&self) -> Result<PreviewObservation> {
        self.runtime.observe_retained_preview(self.owner).map_err(uncertain)
    }
    pub(crate) fn cancel(&self) -> Result<()> {
        if settled(self.observe()?.state()) {
            return Ok(());
        }
        match self.runtime.stop_preview(&self.launch) {
            Ok(_) => Ok(()),
            Err(_) if settled(self.observe()?.state()) => Ok(()),
            Err(error) => Err(uncertain(error)),
        }
    }
    pub(crate) fn input(&self, bytes: &[u8]) -> Result<()> {
        self.control.as_ref().ok_or_else(|| problem("This saved console has no live input attachment; inspect its output or open a new console"))?
            .write_stdin(bytes.to_vec()).map_err(problem)
    }
    pub(crate) fn resize(&self, rows: u16, columns: u16) -> Result<()> {
        self.control
            .as_ref()
            .ok_or_else(|| problem("This saved console has no live resize attachment"))?
            .resize(TerminalSize::new(rows, columns, 0, 0).map_err(problem)?)
            .map_err(problem)
    }
    pub(crate) const fn input_available(&self) -> bool {
        self.control.is_some()
    }
    pub(crate) fn output(
        &self,
        stream: OutputStream,
        offset: u64,
        maximum: usize,
    ) -> Result<(u64, Vec<u8>)> {
        let range = self
            .runtime
            .retained_preview_range(self.owner, stream, offset, maximum)
            .map_err(problem)?;
        Ok((range.total_bytes(), range.bytes().to_vec()))
    }
}

fn scope(key: &str) -> Result<RunId> {
    use sha2::{Digest as _, Sha256};
    let digest = Sha256::digest(format!("peritus/web-owned-command/v1/{key}"));
    let mut bytes = [0; 16];
    bytes.copy_from_slice(&digest[..16]);
    RunId::new(bytes).map_err(|error| problem(format!("Invalid command scope: {error:?}")))
}
fn open_runtime(app: &App, workspace: &Path, source: RunId) -> Result<CommandRuntime> {
    let root = app
        .options
        .state_file
        .parent()
        .ok_or_else(|| problem("No web state directory"))?
        .join("owned-commands")
        .join(hex(source.as_bytes()));
    let processes =
        ProcessStore::open_direct(root.join("processes"), workspace).map_err(problem)?;
    CommandRuntime::open_direct(root.join("runtime"), workspace, source, processes).map_err(problem)
}

/// Reaps settled in-memory observers outside the application map lock; saved bindings remain.
pub fn reap(app: &App) -> Result<()> {
    let commands = app
        .processes
        .lock()
        .map_err(problem)?
        .iter()
        .map(|(key, value)| (key.clone(), Arc::clone(value)))
        .collect::<Vec<_>>();
    let mut completed = Vec::new();
    for (key, command) in commands {
        if command.observe().is_ok_and(|value| settled(value.state())) {
            completed.push((key, command));
        }
    }
    let mut processes = app.processes.lock().map_err(problem)?;
    for (key, observed) in completed {
        if processes.get(&key).is_some_and(|current| Arc::ptr_eq(current, &observed)) {
            processes.remove(&key);
        }
    }
    drop(processes);
    Ok(())
}

pub const fn settled(state: PreviewProcessState) -> bool {
    matches!(
        state,
        PreviewProcessState::Succeeded
            | PreviewProcessState::Failed
            | PreviewProcessState::Cancelled
            | PreviewProcessState::TimedOut
    )
}

pub async fn wait(command: Arc<ManagedCommand>) -> Result<(PreviewProcessState, String, String)> {
    loop {
        let observing = Arc::clone(&command);
        let observed =
            tokio::task::spawn_blocking(move || observing.observe()).await.map_err(problem)??;
        if observed.state() != PreviewProcessState::Running {
            let state = observed.state();
            return tokio::task::spawn_blocking(move || {
                Ok((
                    state,
                    complete_output(&command, OutputStream::Stdout)?,
                    complete_output(&command, OutputStream::Stderr)?,
                ))
            })
            .await
            .map_err(problem)?;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}
fn complete_output(command: &ManagedCommand, stream: OutputStream) -> Result<String> {
    let mut bytes = Vec::new();
    let mut offset = 0;
    loop {
        let (total, page) = command.output(stream, offset, 65536)?;
        offset = offset
            .checked_add(u64::try_from(page.len()).map_err(problem)?)
            .ok_or_else(|| problem("Output offset overflow"))?;
        bytes.extend_from_slice(&page);
        if offset == total {
            break;
        }
        if page.is_empty() {
            return Err(uncertain("Command output stopped before its recorded end"));
        }
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

#[cfg(test)]
mod tests;

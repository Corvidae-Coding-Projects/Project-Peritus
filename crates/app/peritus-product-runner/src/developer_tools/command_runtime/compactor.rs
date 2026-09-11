//! Optional auxiliary inference through the existing C2 authority and native C3 gateway.

mod backend;
mod sandbox;
#[cfg(test)]
mod tests;

use super::{CommandRuntime, authority, contract, identity};
use crate::LocalProcessConfig;
use peritus_process::{
    CommandSpec, DeadlinePolicy, EnvironmentPlan, ExecutionPlan, GracefulAction, IoMode,
    NativeSandboxBackend, OsExitObservation, OutputOverflowAction, OutputPolicy, OutputStream,
    ProcessResourcePolicy, StdinPolicy, TerminalDisposition, WorkingDirectory, WorkspaceAccess,
};
use peritus_sandbox::{AdmissionProfile, admit_backend};
use std::path::Path;

impl CommandRuntime {
    pub(crate) fn compact_local(
        &self,
        config: &LocalProcessConfig,
        input: &[u8],
    ) -> Result<Vec<u8>, String> {
        if input.is_empty() || input.len() > config.max_input_bytes {
            return Err("local compactor input capacity".to_owned());
        }
        if !config.executable.is_file() || !config.model_path.is_file() {
            return Err("local compactor executable or preinstalled weights unavailable".to_owned());
        }
        let executable =
            config.executable.canonicalize().map_err(|_| "resolve local compactor executable")?;
        let weights =
            config.model_path.canonicalize().map_err(|_| "resolve local compactor weights")?;
        if weights.starts_with(&self.inner.workspace_root)
            || executable.starts_with(&self.inner.workspace_root)
            || weights.starts_with(&self.inner.state_root)
            || executable.starts_with(&self.inner.state_root)
        {
            return Err(
                "installed inference inputs must be separate from editable workspace and run state"
                    .to_owned(),
            );
        }
        let ordinal = {
            let mut state =
                self.inner.state.lock().map_err(|_| "local compactor command owner poisoned")?;
            let ordinal = super::ordinal::reserve(
                &self.inner.state_root,
                self.inner.run_id,
                state.next_ordinal,
            )?;
            state.next_ordinal = ordinal;
            ordinal
        };
        let contract = contract::command_contract(self.inner.run_id, ordinal)?;
        let ids = identity::CommandIds::new(self.inner.run_id, ordinal, &contract)?;
        let directory =
            self.inner.state_root.join("local-compactor").join(identity::action_hex(ids.action));
        std::fs::create_dir_all(&directory)
            .map_err(|_| "create local compactor scratch directory")?;
        let directory =
            directory.canonicalize().map_err(|_| "resolve local compactor scratch directory")?;
        let work = create_work_directory(&directory)?;
        let backend = backend::open(config, &work)?;
        self.run_compactor(
            config,
            input,
            &directory,
            (&executable, &weights),
            (&ids, &contract),
            backend,
        )
    }

    fn run_compactor<B: NativeSandboxBackend>(
        &self,
        config: &LocalProcessConfig,
        input: &[u8],
        directory: &Path,
        files: (&Path, &Path),
        authority: (&identity::CommandIds, &peritus_spec::AcceptanceContract),
        backend: B,
    ) -> Result<Vec<u8>, String> {
        let (executable, weights) = files;
        let (ids, contract) = authority;
        let work = directory.join("work");
        let output_bytes = config.max_output_bytes as u64;
        let resources = ProcessResourcePolicy::new(
            config.timeout_millis,
            config.timeout_millis.saturating_mul(4),
            config.memory_bytes,
            16 * 1024 * 1024,
            output_bytes,
            32,
            256,
            1,
        )
        .map_err(detail)?;
        let checked = sandbox::compile(ids, &work, executable, weights, resources)?;
        let admission = admit_backend(&checked, backend.descriptor(), AdmissionProfile::Production)
            .map_err(detail)?;
        let environment = EnvironmentPlan::cleared(Vec::new()).map_err(detail)?;
        let working = WorkingDirectory::open(
            &work,
            ids.workspace,
            ids.resource,
            ids.environment,
            ids.revision.workspace_generation(),
            ids.revision.workspace_revision(),
            WorkspaceAccess::Writable,
        )
        .map_err(detail)?;
        let output = OutputPolicy::new(
            4096.min(output_bytes),
            output_bytes,
            output_bytes,
            8192,
            output_bytes,
            output_bytes,
            output_bytes,
            OutputOverflowAction::Terminate,
        )
        .map_err(detail)?;
        let stdin =
            StdinPolicy::bounded(config.max_input_bytes as u64, config.max_input_bytes as u64)
                .map_err(detail)?;
        let command = CommandSpec::new(
            executable.to_str().ok_or("local executable path is not UTF-8")?.to_owned(),
            [
                "--model".to_owned(),
                weights.to_str().ok_or("local weights path is not UTF-8")?.to_owned(),
            ],
        )
        .map_err(detail)?;
        let plan = ExecutionPlan::new(
            ids.execution_identity(),
            command,
            working,
            environment,
            IoMode::Pipes,
            stdin,
            output,
            DeadlinePolicy::new(Some(config.timeout_millis), GracefulAction::Terminate, 100, 2000)
                .map_err(detail)?,
            resources,
            &checked,
            &admission,
        )
        .map_err(detail)?;
        let process_authority = authority::commit_process(
            &directory.join("authority.sqlite3"),
            ids,
            contract,
            &plan,
            config.timeout_millis,
        )?;
        let authorization = process_authority.request(ids, &plan);
        let process = self
            .inner
            .gateway
            .launch_with_backend(&authorization, plan, &checked, &admission, backend)
            .map_err(detail)?;
        let control = process.control();
        control.write_stdin(input.to_vec()).map_err(detail)?;
        control.close_stdin().map_err(detail)?;
        let terminal = process.wait().map_err(detail)?;
        if terminal.disposition() != TerminalDisposition::Exited
            || terminal.os_exit() != &OsExitObservation::Code(0)
            || !terminal.tree_cleanup_complete()
            || !terminal.support_tasks_joined()
            || !terminal.output().is_complete()
        {
            return Err(format!(
                "local compactor did not produce a complete successful result: {:?}",
                terminal.disposition()
            ));
        }
        let bytes = control.retained_stream_output(OutputStream::Stdout);
        if bytes.is_empty() || bytes.len() > config.max_output_bytes {
            return Err("local compactor output capacity".to_owned());
        }
        Ok(bytes)
    }
}

fn detail(error: impl std::fmt::Display) -> String {
    format!("local compactor: {error}")
}

fn create_work_directory(directory: &Path) -> Result<std::path::PathBuf, String> {
    let work = directory.join("work");
    std::fs::create_dir(&work).map_err(|_| "create isolated compactor working directory")?;
    // C3 protects these metadata names even in scratch workspaces. Materialize them before
    // admitting descendant creation so Linux can mask them without weakening that invariant.
    for name in [".git", ".peritus", ".crosslink"] {
        std::fs::create_dir(work.join(name)).map_err(|_| "create protected compactor metadata")?;
    }
    Ok(work)
}

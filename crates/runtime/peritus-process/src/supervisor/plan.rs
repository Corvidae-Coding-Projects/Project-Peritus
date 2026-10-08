//! Exact process-supervision projection of an accepted execution plan.

use std::path::{Path, PathBuf};

use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    BackendResourceFidelity, CommandSpec, DeadlinePolicy, EnvironmentPlan, ExecutionPlan, IoMode,
    OutputPolicy, ProcessResourcePolicy, StdinPolicy, TerminalCapabilities,
};

/// Moveable projection containing every field consulted after one-use authority consumption.
///
/// The complete canonical [`ExecutionPlan`] remains the admission authority. This projection is
/// deliberately smaller: it can be retained by an independent process owner without reopening
/// policy or reconstructing a second authorization decision.
#[derive(Clone, Debug)]
pub(crate) struct SupervisorPlan {
    process_id: ProcessId,
    plan_digest: Sha256Digest,
    command: CommandSpec,
    working_directory: PathBuf,
    environment: EnvironmentPlan,
    io_mode: IoMode,
    stdin: StdinPolicy,
    terminal: TerminalCapabilities,
    output: OutputPolicy,
    deadlines: DeadlinePolicy,
    resources: ProcessResourcePolicy,
    sandbox_digest: Sha256Digest,
    backend_descriptor_digest: Sha256Digest,
    backend_resource_fidelity: BackendResourceFidelity,
}

impl SupervisorPlan {
    pub(crate) fn from_execution(plan: &ExecutionPlan) -> Self {
        Self {
            process_id: plan.identity().process_id(),
            plan_digest: plan.digest(),
            command: plan.command().clone(),
            working_directory: plan.working_directory().path().to_path_buf(),
            environment: plan.environment().clone(),
            io_mode: plan.io_mode(),
            stdin: plan.stdin_policy(),
            terminal: plan.terminal_capabilities(),
            output: plan.output_policy(),
            deadlines: plan.deadline_policy(),
            resources: plan.resource_policy(),
            sandbox_digest: plan.sandbox_digest(),
            backend_descriptor_digest: plan.backend().descriptor_digest(),
            backend_resource_fidelity: plan.backend().resource_fidelity(),
        }
    }

    pub(crate) const fn process_id(&self) -> ProcessId {
        self.process_id
    }

    pub(crate) const fn digest(&self) -> Sha256Digest {
        self.plan_digest
    }

    pub(crate) const fn command(&self) -> &CommandSpec {
        &self.command
    }

    pub(crate) fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    pub(crate) const fn environment(&self) -> &EnvironmentPlan {
        &self.environment
    }

    pub(crate) const fn io_mode(&self) -> IoMode {
        self.io_mode
    }

    pub(crate) const fn stdin_policy(&self) -> StdinPolicy {
        self.stdin
    }

    pub(crate) const fn terminal_capabilities(&self) -> TerminalCapabilities {
        self.terminal
    }

    pub(crate) const fn output_policy(&self) -> OutputPolicy {
        self.output
    }

    pub(crate) const fn deadline_policy(&self) -> DeadlinePolicy {
        self.deadlines
    }

    pub(crate) const fn resource_policy(&self) -> ProcessResourcePolicy {
        self.resources
    }

    pub(crate) const fn sandbox_digest(&self) -> Sha256Digest {
        self.sandbox_digest
    }

    pub(crate) const fn backend_descriptor_digest(&self) -> Sha256Digest {
        self.backend_descriptor_digest
    }

    pub(crate) const fn backend_resource_fidelity(&self) -> BackendResourceFidelity {
        self.backend_resource_fidelity
    }
}

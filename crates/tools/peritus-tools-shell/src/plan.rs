//! Complete C2 execution-plan compiler inputs.

use peritus_process::{
    CommandSpec, DeadlinePolicy, EnvironmentPlan, ExecutionCallerBinding, ExecutionIdentity,
    ExecutionPlan, IoMode, OutputPolicy, ProcessResourcePolicy, StdinPolicy, WorkingDirectory,
};
use peritus_sandbox::{
    BackendAdmission, CheckedSandboxPlan, IsolationRequirement, SandboxOperationClass,
};
use peritus_tool_protocol::PreparedToolCall;

use crate::{ShellError, ShellErrorKind};

/// Exact non-command inputs required to compile one restricted C2 execution plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionPlanInputs {
    /// Complete lifecycle and workspace identity.
    pub identity: ExecutionIdentity,
    /// Opened exact working directory and access mode.
    pub working_directory: WorkingDirectory,
    /// Resolved deterministic child environment.
    pub environment: EnvironmentPlan,
    /// Pipe or PTY selection.
    pub io_mode: IoMode,
    /// Bounded standard-input policy.
    pub stdin: StdinPolicy,
    /// Retention, rendering, and event bounds.
    pub output: OutputPolicy,
    /// Wall deadline and graceful/escalation behavior.
    pub deadlines: DeadlinePolicy,
    /// Complete C2 resource ceilings.
    pub resources: ProcessResourcePolicy,
}

impl ExecutionPlanInputs {
    /// Compiles one complete restricted execution plan.
    ///
    /// Network, secret, filesystem, process, terminal, recovery, and backend projections remain
    /// bound by the exact checked sandbox plan and admission. No raw-effect fallback is attempted.
    ///
    /// # Errors
    /// Returns a typed failure unless the supplied plan is restricted execution or C2 rejects an
    /// identity, projection, bound, or backend mismatch.
    pub fn compile(
        &self,
        prepared: &PreparedToolCall,
        caller_binding: ExecutionCallerBinding,
        command: CommandSpec,
        sandbox: &CheckedSandboxPlan,
        admission: &BackendAdmission,
    ) -> Result<ExecutionPlan, ShellError> {
        if sandbox.isolation() != IsolationRequirement::Restricted
            || sandbox.operation_class() != SandboxOperationClass::Execution
        {
            return Err(ShellError::new(
                ShellErrorKind::InvalidInput,
                "shell tools require a restricted execution sandbox plan",
            ));
        }
        if caller_binding.action_id() != prepared.call().action_id()
            || caller_binding.capability_name() != prepared.descriptor().name()
            || caller_binding.descriptor_digest() != prepared.descriptor_digest().get()
            || caller_binding.prepared_digest() != prepared.prepared_digest()
            || caller_binding.actor_id() != self.identity.actor_id()
            || caller_binding.environment_id() != self.identity.environment_id()
            || caller_binding.resource_id() != self.identity.resource_id()
            || prepared.call().revision() != self.identity.revision()
        {
            return Err(ShellError::new(
                ShellErrorKind::InvocationMismatch,
                "C4 caller target binding differs from the prepared shell call",
            ));
        }
        let plan = ExecutionPlan::new(
            self.identity,
            command,
            self.working_directory.clone(),
            self.environment.clone(),
            self.io_mode,
            self.stdin,
            self.output,
            self.deadlines,
            self.resources,
            sandbox,
            admission,
        )?;
        plan.bind_caller(caller_binding).map_err(Into::into)
    }
}

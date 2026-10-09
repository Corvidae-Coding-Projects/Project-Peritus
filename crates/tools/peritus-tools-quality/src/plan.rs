//! Complete restricted C2 plan compilation for one selected check.

use peritus_process::{
    DeadlinePolicy, EnvironmentPlan, ExecutionCallerBinding, ExecutionIdentity, ExecutionPlan,
    IoMode, OutputPolicy, ProcessResourcePolicy, StdinPolicy, WorkingDirectory,
};
use peritus_sandbox::{
    BackendAdmission, CheckedSandboxPlan, IsolationRequirement, SandboxOperationClass,
};
use peritus_tool_protocol::PreparedToolCall;

use crate::{CheckDefinition, EnvironmentProfile, QualityError, QualityErrorKind};

/// Exact non-command C2 inputs and resolved environment profile for a quality run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QualityPlanInputs {
    /// Complete lifecycle and workspace identity.
    pub identity: ExecutionIdentity,
    /// Opened exact check working directory.
    pub working_directory: WorkingDirectory,
    /// Resolved deterministic child environment.
    pub environment: EnvironmentPlan,
    /// Stable name of the resolved environment profile.
    pub environment_profile: EnvironmentProfile,
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

impl QualityPlanInputs {
    /// Compiles a selected definition into one complete restricted C2 plan.
    ///
    /// # Errors
    /// Returns a typed failure for profile/deadline/output mismatch, unrestricted sandbox input,
    /// or any C2 identity, projection, bound, or backend rejection.
    pub fn compile(
        &self,
        prepared: &PreparedToolCall,
        caller_binding: ExecutionCallerBinding,
        definition: &CheckDefinition,
        sandbox: &CheckedSandboxPlan,
        admission: &BackendAdmission,
    ) -> Result<ExecutionPlan, QualityError> {
        if definition.environment_profile() != &self.environment_profile {
            return Err(invalid("resolved environment profile differs from the check definition"));
        }
        if self.deadlines.wall_timeout_millis() != Some(definition.timeout_millis()) {
            return Err(invalid("execution deadline differs from the check definition"));
        }
        if self.output.spool_bytes() < definition.output_bytes() {
            return Err(invalid("execution output retention is below the check definition bound"));
        }
        if sandbox.isolation() != IsolationRequirement::Restricted
            || sandbox.operation_class() != SandboxOperationClass::Execution
        {
            return Err(invalid("quality runs require a restricted execution sandbox plan"));
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
            return Err(QualityError::new(
                QualityErrorKind::InvocationMismatch,
                "C4 caller target binding differs from the prepared quality call",
            ));
        }
        let plan = ExecutionPlan::new(
            self.identity,
            definition.command()?,
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

fn invalid(detail: &'static str) -> QualityError {
    QualityError::new(QualityErrorKind::InvalidInput, detail)
}

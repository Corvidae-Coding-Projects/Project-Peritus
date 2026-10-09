//! Complete canonical execution plan identity.

use sha2::{Digest, Sha256};

use peritus_policy::OperationClass;
use peritus_sandbox::{
    BackendAdmission, BackendKind, CheckedSandboxPlan, IsolationRequirement,
    ResourceFidelity as SandboxResourceFidelity, SandboxOperationClass,
};
use peritus_types::Sha256Digest;

use crate::{
    CommandSpec, DeadlinePolicy, EnvironmentPlan, ExecutionCallerBinding, ExecutionIdentity,
    IoMode, OutputPolicy, ProcessError, ProcessResourcePolicy, StdinPolicy, TerminalCapabilities,
    WorkingDirectory, error::invalid,
};

mod projection;

use projection::validate_sandbox_projection;

const MAX_BACKEND_TOKEN_BYTES: usize = 128;

/// Whether the exact plan requests isolation or an explicitly authorized raw effect.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ExecutionIsolation {
    /// Complete required sandbox controls must be native and enforced.
    Restricted,
    /// A separately authorized raw-effect operation accepts local process containment only.
    ExplicitRawEffect,
}

/// Resource enforcement fidelity declared by the selected sandbox backend.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BackendResourceFidelity {
    /// Kernel or equivalent hard enforcement.
    Hard,
    /// Supervisor sampling with deterministic cancellation on a crossed ceiling.
    Supervisor,
    /// In-memory reference accounting that cannot authorize an operating-system effect.
    Reference,
}

impl ExecutionIsolation {
    /// Returns the exact required B1 operation class.
    #[must_use]
    pub const fn operation_class(self) -> OperationClass {
        match self {
            Self::Restricted => OperationClass::Execution,
            Self::ExplicitRawEffect => OperationClass::RawEffect,
        }
    }
}

/// Digest-bound selected sandbox backend and preparation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendSelection {
    name: String,
    version: String,
    native: bool,
    resource_fidelity: BackendResourceFidelity,
    descriptor_digest: Sha256Digest,
    support_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
}

impl BackendSelection {
    /// Projects one fail-closed sandbox admission into the execution identity.
    ///
    /// # Errors
    ///
    /// Returns an error when the admission does not bind the supplied checked plan or its backend
    /// identity cannot be represented by execution plan version one.
    pub fn from_admission(
        plan: &CheckedSandboxPlan,
        admission: &BackendAdmission,
    ) -> Result<Self, ProcessError> {
        if admission.plan_digest() != plan.digest() {
            return Err(invalid("sandbox admission differs from the checked plan"));
        }
        let descriptor = admission.descriptor();
        let name = descriptor.name().as_str().to_owned();
        let version = descriptor.version().as_str().to_owned();
        if !valid_backend_token(&name) || !valid_backend_token(&version) {
            return Err(invalid("backend name or version is invalid or exceeds its bound"));
        }
        Ok(Self {
            name,
            version,
            native: descriptor.kind() == BackendKind::Native,
            resource_fidelity: match descriptor.resource_fidelity() {
                SandboxResourceFidelity::Hard => BackendResourceFidelity::Hard,
                SandboxResourceFidelity::Supervisor => BackendResourceFidelity::Supervisor,
                SandboxResourceFidelity::Reference => BackendResourceFidelity::Reference,
            },
            descriptor_digest: admission.descriptor_digest(),
            support_digest: admission.support_digest(),
            preparation_digest: admission.preparation_digest(),
        })
    }

    /// Returns the backend stable name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    /// Returns the backend stable version.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
    /// Returns whether this is a native enforcement backend.
    #[must_use]
    pub const fn is_native(&self) -> bool {
        self.native
    }
    /// Returns the backend's admitted resource-enforcement fidelity.
    #[must_use]
    pub const fn resource_fidelity(&self) -> BackendResourceFidelity {
        self.resource_fidelity
    }
    /// Returns the complete descriptor digest.
    #[must_use]
    pub const fn descriptor_digest(&self) -> Sha256Digest {
        self.descriptor_digest
    }
    /// Returns the admitted support-set digest.
    #[must_use]
    pub const fn support_digest(&self) -> Sha256Digest {
        self.support_digest
    }
    /// Returns the exact preparation digest expected at activation.
    #[must_use]
    pub const fn preparation_digest(&self) -> Sha256Digest {
        self.preparation_digest
    }
}

/// Complete checked execution plan consumed by the authorization gateway.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionPlan {
    identity: ExecutionIdentity,
    command: CommandSpec,
    working_directory: WorkingDirectory,
    environment: EnvironmentPlan,
    io_mode: IoMode,
    stdin: StdinPolicy,
    terminal: TerminalCapabilities,
    output: OutputPolicy,
    deadlines: DeadlinePolicy,
    resources: ProcessResourcePolicy,
    caller_binding: Option<ExecutionCallerBinding>,
    isolation: ExecutionIsolation,
    sandbox_digest: Sha256Digest,
    backend: BackendSelection,
    canonical: Vec<u8>,
    digest: Sha256Digest,
}

impl ExecutionPlan {
    /// Validates cross-field identities and freezes complete canonical version-one bytes.
    ///
    /// # Errors
    ///
    /// Returns an error for a target mismatch, inconsistent resource/output/deadline bounds, a
    /// restricted plan using a non-native backend, or canonical length overflow.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        identity: ExecutionIdentity,
        command: CommandSpec,
        working_directory: WorkingDirectory,
        environment: EnvironmentPlan,
        io_mode: IoMode,
        stdin: StdinPolicy,
        output: OutputPolicy,
        deadlines: DeadlinePolicy,
        resources: ProcessResourcePolicy,
        sandbox_plan: &CheckedSandboxPlan,
        admission: &BackendAdmission,
    ) -> Result<Self, ProcessError> {
        let isolation = match sandbox_plan.isolation() {
            IsolationRequirement::Restricted => ExecutionIsolation::Restricted,
            IsolationRequirement::ExplicitRawEffect => ExecutionIsolation::ExplicitRawEffect,
        };
        let expected_class = match sandbox_plan.operation_class() {
            SandboxOperationClass::Execution => OperationClass::Execution,
            SandboxOperationClass::RawEffect => OperationClass::RawEffect,
        };
        if expected_class != isolation.operation_class() {
            return Err(invalid("sandbox isolation and operation class disagree"));
        }
        let backend = BackendSelection::from_admission(sandbox_plan, admission)?;
        let sandbox_digest = sandbox_plan.digest();
        let revision = identity.revision();
        let directory_generation = working_directory.generation();
        let exact_target = identity.workspace_id() == working_directory.workspace_id()
            && identity.resource_id() == working_directory.resource_id()
            && identity.environment_id() == working_directory.environment_id()
            && revision.workspace_id() == working_directory.workspace_id()
            && revision.workspace_generation() == directory_generation
            && revision.workspace_revision() == working_directory.revision();
        if !exact_target {
            return Err(invalid("working directory differs from the complete execution identity"));
        }
        let binding = sandbox_plan.binding();
        if binding.process_id() != identity.process_id()
            || binding.resource_id() != identity.resource_id()
            || binding.environment_id() != identity.environment_id()
            || binding.revision() != identity.revision()
        {
            return Err(invalid("checked sandbox target differs from the execution identity"));
        }
        let terminal = validate_sandbox_projection(
            sandbox_plan,
            &environment,
            io_mode,
            stdin,
            output,
            resources,
        )?;
        if isolation == ExecutionIsolation::Restricted && !backend.is_native() {
            return Err(ProcessError::new(
                crate::ErrorCode::Unsupported,
                crate::ProcessOperation::Validate,
                crate::RecoveryClass::SelectBackend,
                "restricted execution requires a native admitted backend",
            ));
        }
        if deadlines
            .wall_timeout_millis()
            .is_some_and(|deadline| deadline > resources.wall_millis())
            || output.spool_bytes() > resources.output_bytes()
            || output.stdout_bytes() > resources.output_bytes()
            || output.stderr_bytes() > resources.output_bytes()
            || output.terminal_bytes() > resources.output_bytes()
        {
            return Err(invalid("deadline or output policy exceeds the resource policy"));
        }
        let mut plan = Self {
            identity,
            command,
            working_directory,
            environment,
            io_mode,
            stdin,
            terminal,
            output,
            deadlines,
            resources,
            caller_binding: None,
            isolation,
            sandbox_digest,
            backend,
            canonical: Vec::new(),
            digest: Sha256Digest::new([0; 32]),
        };
        plan.canonical = crate::plan_canonical::encode(&plan)?;
        plan.digest = Sha256Digest::new(Sha256::digest(&plan.canonical).into());
        Ok(plan)
    }

    /// Returns the complete owner identity.
    #[must_use]
    pub const fn identity(&self) -> ExecutionIdentity {
        self.identity
    }
    /// Returns the structured command.
    #[must_use]
    pub const fn command(&self) -> &CommandSpec {
        &self.command
    }
    /// Returns the checked working directory.
    #[must_use]
    pub const fn working_directory(&self) -> &WorkingDirectory {
        &self.working_directory
    }
    /// Returns the deterministic environment.
    #[must_use]
    pub const fn environment(&self) -> &EnvironmentPlan {
        &self.environment
    }
    /// Returns the selected pipe or PTY mode.
    #[must_use]
    pub const fn io_mode(&self) -> IoMode {
        self.io_mode
    }
    /// Returns the input policy.
    #[must_use]
    pub const fn stdin_policy(&self) -> StdinPolicy {
        self.stdin
    }
    /// Returns terminal control authority and observation bounds.
    #[must_use]
    pub const fn terminal_capabilities(&self) -> TerminalCapabilities {
        self.terminal
    }
    /// Returns output bounds.
    #[must_use]
    pub const fn output_policy(&self) -> OutputPolicy {
        self.output
    }
    /// Returns deadline/escalation policy.
    #[must_use]
    pub const fn deadline_policy(&self) -> DeadlinePolicy {
        self.deadlines
    }
    /// Returns process resource ceilings.
    #[must_use]
    pub const fn resource_policy(&self) -> ProcessResourcePolicy {
        self.resources
    }
    /// Returns an optional higher-layer invocation identity bound into this plan.
    #[must_use]
    pub const fn caller_binding(&self) -> Option<&ExecutionCallerBinding> {
        self.caller_binding.as_ref()
    }

    /// Binds one exact higher-layer invocation and recomputes canonical plan identity.
    ///
    /// # Errors
    /// Returns a typed failure if a binding already exists or canonical encoding fails.
    pub fn bind_caller(mut self, binding: ExecutionCallerBinding) -> Result<Self, ProcessError> {
        if self.caller_binding.is_some() {
            return Err(invalid("execution plan already has a caller binding"));
        }
        self.caller_binding = Some(binding);
        self.canonical = crate::plan_canonical::encode(&self)?;
        self.digest = Sha256Digest::new(Sha256::digest(&self.canonical).into());
        Ok(self)
    }
    /// Returns isolation class.
    #[must_use]
    pub const fn isolation(&self) -> ExecutionIsolation {
        self.isolation
    }
    /// Returns the exact checked sandbox digest.
    #[must_use]
    pub const fn sandbox_digest(&self) -> Sha256Digest {
        self.sandbox_digest
    }
    /// Returns the selected backend binding.
    #[must_use]
    pub const fn backend(&self) -> &BackendSelection {
        &self.backend
    }
    /// Borrows complete canonical version-one bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }
    /// Returns the SHA-256 digest of complete canonical bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn valid_backend_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_BACKEND_TOKEN_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

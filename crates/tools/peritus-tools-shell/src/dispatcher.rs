//! Router-permit-only shell dispatch through the restricted native C2 gateway.

use peritus_artifact_store::ArtifactStore;
use peritus_process::{
    ExecutionAuthorizationRequest, ExecutionGateway, ExecutionIsolation, ExecutionPlan, IoMode,
    NativeSandboxBackend,
};
use peritus_sandbox::{BackendAdmission, CheckedSandboxPlan};
use peritus_tool_protocol::{ImplementationIdentity, SchemaDigest};
use peritus_tool_router::{AuthorizedInvocation, DispatchFailure, ToolDispatcher, ToolStart};

use crate::execution::failure;
use crate::{
    ExecInput, ScriptInput, ShellError, ShellErrorKind, ShellExecution, exec_descriptor,
    script_descriptor,
};

/// One-use dispatcher bound to exact C2 authority, plan, C3 admission/backend, and artifact store.
pub struct ShellDispatcher<'gateway, 'authority, B> {
    gateway: &'gateway ExecutionGateway,
    authorization: &'gateway ExecutionAuthorizationRequest<'authority>,
    plan: Option<ExecutionPlan>,
    sandbox: CheckedSandboxPlan,
    admission: BackendAdmission,
    backend: Option<B>,
    artifacts: Option<ArtifactStore>,
    descriptor: peritus_tool_protocol::ToolDescriptor,
}

impl<'gateway, 'authority, B> ShellDispatcher<'gateway, 'authority, B>
where
    B: NativeSandboxBackend,
{
    /// Binds all lower-layer authority and one exact precompiled C4-linked C2 plan.
    ///
    /// # Errors
    /// Returns a typed failure unless the plan is restricted and its caller binding names and
    /// hashes exactly one canonical shell descriptor.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gateway: &'gateway ExecutionGateway,
        authorization: &'gateway ExecutionAuthorizationRequest<'authority>,
        plan: ExecutionPlan,
        sandbox: CheckedSandboxPlan,
        admission: BackendAdmission,
        backend: B,
        artifacts: ArtifactStore,
    ) -> Result<Self, ShellError> {
        let binding = plan
            .caller_binding()
            .ok_or_else(|| mismatch("shell execution plan has no C4 caller binding"))?;
        let descriptor = match binding.capability_name().as_str() {
            "shell.exec" => exec_descriptor()?,
            "shell.script" => script_descriptor()?,
            _ => return Err(mismatch("shell plan names another tool")),
        };
        if plan.isolation() != ExecutionIsolation::Restricted
            || descriptor.descriptor_digest().get() != binding.descriptor_digest()
            || sandbox.digest() != plan.sandbox_digest()
            || admission.plan_digest() != sandbox.digest()
        {
            return Err(mismatch(
                "shell descriptor, restricted plan, sandbox, or admission binding differs",
            ));
        }
        Ok(Self {
            gateway,
            authorization,
            plan: Some(plan),
            sandbox,
            admission,
            backend: Some(backend),
            artifacts: Some(artifacts),
            descriptor,
        })
    }

    fn validate_invocation(
        &self,
        invocation: &AuthorizedInvocation,
        plan: &ExecutionPlan,
    ) -> Result<(), DispatchFailure> {
        let prepared = invocation.prepared();
        let binding = plan.caller_binding().ok_or_else(|| {
            failure::adapter(
                "shell-caller-binding",
                "execution plan lost its exact C4 caller binding",
            )
        })?;
        let permit = invocation.binding();
        let identity = plan.identity();
        let exact = prepared.descriptor().name() == binding.capability_name()
            && prepared.call().action_id() == binding.action_id()
            && prepared.descriptor_digest().get() == binding.descriptor_digest()
            && prepared.prepared_digest() == binding.prepared_digest()
            && prepared.descriptor_digest() == self.descriptor.descriptor_digest()
            && permit.actor_id() == binding.actor_id()
            && permit.role() == binding.role()
            && permit.environment_id() == binding.environment_id()
            && permit.resource_id() == binding.resource_id()
            && permit.revision() == prepared.call().revision()
            && identity.revision() == permit.revision()
            && identity.actor_id() == binding.actor_id()
            && identity.environment_id() == binding.environment_id()
            && identity.resource_id() == binding.resource_id();
        if !exact {
            return Err(failure::adapter(
                "shell-invocation-mismatch",
                "router invocation differs from the C2 caller-bound execution plan",
            ));
        }
        let expected_command =
            match prepared.descriptor().name().as_str() {
                "shell.exec" => ExecInput::from_arguments(prepared.arguments())
                    .and_then(|input| input.command()),
                "shell.script" => ScriptInput::from_arguments(prepared.arguments())
                    .and_then(|input| input.command()),
                _ => Err(mismatch("authorized invocation names another tool")),
            }
            .map_err(|error| failure::adapter("shell-input", error.detail()))?;
        if &expected_command != plan.command()
            || plan.deadline_policy().wall_timeout_millis()
                != Some(prepared.call().limits().timeout_millis())
            || plan.output_policy().spool_bytes() > prepared.call().limits().output_bytes()
            || plan.output_policy().stdout_bytes() > prepared.call().limits().output_bytes()
            || plan.output_policy().stderr_bytes() > prepared.call().limits().output_bytes()
            || plan.output_policy().terminal_bytes() > prepared.call().limits().output_bytes()
        {
            return Err(failure::adapter(
                "shell-plan-mismatch",
                "argv, deadline, or output bounds differ from the authorized tool call",
            ));
        }
        let required_artifacts = match plan.io_mode() {
            IoMode::Pipes => 2,
            IoMode::Pty(_) => 1,
        };
        if prepared.call().limits().artifacts() < required_artifacts {
            return Err(failure::adapter(
                "shell-artifact-limit",
                "call artifact bound cannot represent every possible C2 output stream",
            ));
        }
        Ok(())
    }
}

impl<B> ToolDispatcher for ShellDispatcher<'_, '_, B>
where
    B: NativeSandboxBackend,
{
    fn implementation_identity(&self) -> &ImplementationIdentity {
        self.descriptor.implementation_identity()
    }

    fn descriptor_digest(&self) -> SchemaDigest {
        self.descriptor.descriptor_digest()
    }

    fn start(&mut self, invocation: AuthorizedInvocation) -> Result<ToolStart, DispatchFailure> {
        let plan = self.plan.as_ref().ok_or_else(|| {
            failure::adapter(
                "shell-already-consumed",
                "shell dispatcher lower-layer resources were already consumed",
            )
        })?;
        self.validate_invocation(&invocation, plan)?;
        let creating_event = invocation.dispatch_event();
        let started_at = invocation.observed_at();
        let prepared = invocation.into_prepared();
        let plan = self.plan.take().ok_or_else(|| {
            failure::adapter("shell-already-consumed", "shell execution plan was already consumed")
        })?;
        let backend = self.backend.take().ok_or_else(|| {
            failure::adapter("shell-already-consumed", "shell native backend was already consumed")
        })?;
        let artifacts = self.artifacts.take().ok_or_else(|| {
            failure::adapter("shell-already-consumed", "shell artifact store was already consumed")
        })?;
        let owner = self
            .gateway
            .launch_with_backend(self.authorization, plan, &self.sandbox, &self.admission, backend)
            .map_err(|error| failure::process(&error))?;
        Ok(ToolStart::Active(Box::new(ShellExecution::new(
            prepared,
            owner,
            artifacts,
            creating_event,
            started_at,
        ))))
    }
}

fn mismatch(detail: &'static str) -> ShellError {
    ShellError::new(ShellErrorKind::InvocationMismatch, detail)
}

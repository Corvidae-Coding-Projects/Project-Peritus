//! Authorized quality check dispatch through restricted native C2/C3 execution.

use peritus_artifact_store::ArtifactStore;
use peritus_process::{
    ExecutionAuthorizationRequest, ExecutionGateway, ExecutionIsolation, ExecutionPlan, IoMode,
    NativeSandboxBackend,
};
use peritus_sandbox::{BackendAdmission, CheckedSandboxPlan};
use peritus_tool_protocol::{ImplementationIdentity, SchemaDigest};
use peritus_tool_router::{AuthorizedInvocation, DispatchFailure, ToolDispatcher, ToolStart};

use super::adapter_failure;
use crate::execution::failure;
use crate::{
    CheckCatalog, QualityError, QualityErrorKind, RunInput, execution::QualityExecution,
    run_descriptor,
};

/// One-use exact-check dispatcher bound to C2 authority and one native C3 backend.
pub struct QualityRunDispatcher<'gateway, 'authority, B> {
    gateway: &'gateway ExecutionGateway,
    authorization: &'gateway ExecutionAuthorizationRequest<'authority>,
    plan: Option<ExecutionPlan>,
    sandbox: CheckedSandboxPlan,
    admission: BackendAdmission,
    backend: Option<B>,
    artifacts: Option<ArtifactStore>,
    catalog: CheckCatalog,
    descriptor: peritus_tool_protocol::ToolDescriptor,
}

impl<'gateway, 'authority, B> QualityRunDispatcher<'gateway, 'authority, B>
where
    B: NativeSandboxBackend,
{
    /// Binds a discovered catalog and exact restricted lower-layer execution resources.
    ///
    /// # Errors
    /// Returns a typed failure unless the plan carries the canonical `quality.run` caller binding
    /// and exactly matches its checked sandbox/admission.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        gateway: &'gateway ExecutionGateway,
        authorization: &'gateway ExecutionAuthorizationRequest<'authority>,
        plan: ExecutionPlan,
        sandbox: CheckedSandboxPlan,
        admission: BackendAdmission,
        backend: B,
        artifacts: ArtifactStore,
        catalog: CheckCatalog,
    ) -> Result<Self, QualityError> {
        let descriptor = run_descriptor()?;
        let binding = plan
            .caller_binding()
            .ok_or_else(|| mismatch("quality execution plan has no C4 caller binding"))?;
        if plan.isolation() != ExecutionIsolation::Restricted
            || binding.capability_name().as_str() != "quality.run"
            || binding.descriptor_digest() != descriptor.descriptor_digest().get()
            || sandbox.digest() != plan.sandbox_digest()
            || admission.plan_digest() != sandbox.digest()
        {
            return Err(mismatch(
                "quality descriptor, restricted plan, sandbox, or admission binding differs",
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
            catalog,
            descriptor,
        })
    }

    fn validate(
        &self,
        invocation: &AuthorizedInvocation,
        plan: &ExecutionPlan,
    ) -> Result<crate::CheckDefinition, DispatchFailure> {
        let prepared = invocation.prepared();
        let binding = plan.caller_binding().ok_or_else(|| {
            adapter_failure("quality-caller-binding", "execution plan lost its C4 caller binding")
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
            return Err(adapter_failure(
                "quality-invocation-mismatch",
                "router permit differs from the C2 caller-bound quality plan",
            ));
        }
        let input = RunInput::from_arguments(prepared.arguments())
            .map_err(|error| adapter_failure("quality-run-input", error.detail()))?;
        let definition = self.catalog.find(input.gate_name()).cloned().ok_or_else(|| {
            adapter_failure(
                "quality-unknown-check",
                "selected gate is absent from the authorized combined catalog",
            )
        })?;
        let command = definition
            .command()
            .map_err(|error| adapter_failure("quality-command", error.detail()))?;
        if &command != plan.command()
            || plan.deadline_policy().wall_timeout_millis() != Some(definition.timeout_millis())
            || plan.deadline_policy().wall_timeout_millis()
                != Some(prepared.call().limits().timeout_millis())
            || plan.output_policy().spool_bytes() < definition.output_bytes()
            || plan.output_policy().spool_bytes() > prepared.call().limits().output_bytes()
        {
            return Err(adapter_failure(
                "quality-plan-mismatch",
                "selected definition, argv, deadline, or output differs from the C2 plan",
            ));
        }
        let required_artifacts = match plan.io_mode() {
            IoMode::Pipes => 2,
            IoMode::Pty(_) => 1,
        };
        if prepared.call().limits().artifacts() < required_artifacts {
            return Err(adapter_failure(
                "quality-artifact-limit",
                "call artifact bound cannot represent every possible C2 output stream",
            ));
        }
        Ok(definition)
    }
}

impl<B> ToolDispatcher for QualityRunDispatcher<'_, '_, B>
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
            adapter_failure("quality-run-consumed", "quality run dispatcher was already consumed")
        })?;
        let definition = self.validate(&invocation, plan)?;
        let creating_event = invocation.dispatch_event();
        let started_at = invocation.observed_at();
        let prepared = invocation.into_prepared();
        let plan = self.plan.take().ok_or_else(|| {
            adapter_failure("quality-run-consumed", "quality execution plan was already consumed")
        })?;
        let backend = self.backend.take().ok_or_else(|| {
            adapter_failure("quality-run-consumed", "quality native backend was already consumed")
        })?;
        let artifacts = self.artifacts.take().ok_or_else(|| {
            adapter_failure("quality-run-consumed", "quality artifact store was already consumed")
        })?;
        let owner = self
            .gateway
            .launch_with_backend(self.authorization, plan, &self.sandbox, &self.admission, backend)
            .map_err(|error| failure::process(&error))?;
        Ok(ToolStart::Active(Box::new(QualityExecution::new(
            prepared,
            definition,
            owner,
            artifacts,
            creating_event,
            started_at,
        ))))
    }
}

fn mismatch(detail: &'static str) -> QualityError {
    QualityError::new(QualityErrorKind::InvocationMismatch, detail)
}

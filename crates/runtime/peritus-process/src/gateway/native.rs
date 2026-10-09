//! Authorized restricted execution through one exact native backend.

use peritus_sandbox::{BackendAdmission, BackendKind, CheckedSandboxPlan};

use super::{AuthorizedLaunch, ExecutionGateway, ExecutionPermit, validate_request};
use crate::{
    AuthorizedPreparationContext, ErrorCode, ExecutionAuthorizationRequest, ExecutionPlan,
    NativePlatform, NativeSandboxBackend, NativeSandboxSession, OwnedProcess, ProcessError,
    ProcessOperation, RecoveryClass, supervisor,
};

impl ExecutionGateway {
    /// Validates, durably consumes, and starts one restricted execution through a native backend.
    ///
    /// The backend is inspected before consumption but receives its opaque preparation context
    /// only after the existing complete authority check and durable one-use consume. Its prepared
    /// session is then retained by the ordinary C2 supervisor through target termination and
    /// backend release.
    ///
    /// # Errors
    ///
    /// Returns a stable typed error before consumption for any authority, platform, descriptor,
    /// plan, or admission mismatch. Preparation failure after consumption is durably recorded as
    /// a non-success terminal result before the error is returned.
    pub fn launch_with_backend<B>(
        &self,
        request: &ExecutionAuthorizationRequest<'_>,
        plan: ExecutionPlan,
        sandbox_plan: &CheckedSandboxPlan,
        admission: &BackendAdmission,
        backend: B,
    ) -> Result<OwnedProcess, ProcessError>
    where
        B: NativeSandboxBackend,
    {
        validate_native_binding(&plan, sandbox_plan, admission, &backend)?;
        let validation = validate_request(request, &plan)?;
        supervisor::validate_native_launch(&plan)?;
        let permit = ExecutionPermit {
            _action_id: plan.identity().action_id(),
            _process_id: plan.identity().process_id(),
            action_digest: validation.action_digest,
            _plan_digest: plan.digest(),
        };
        self.store.consume(&plan, validation.action_digest, validation.lease_claim)?;
        let context = AuthorizedPreparationContext::new(&plan, sandbox_plan, admission);
        let mut session = match backend.prepare(context) {
            Ok(session) => session,
            Err(error) => {
                // A failed preparation cannot return an owned session whose release can be
                // checked. Preserve incomplete cleanup evidence instead of inferring success from
                // Rust drops whose cleanup errors are intentionally unobservable here.
                supervisor::record_preparation_failure(&self.store, &plan, false)?;
                return Err(error);
            }
        };
        if let Err(validation_error) =
            crate::native::validate_prepared_session(&session, &plan, sandbox_plan)
        {
            let release_error = session.release().err();
            let cleanup_complete = release_error.is_none()
                && crate::native::validate_released_session(&session, &plan, sandbox_plan.digest())
                    .is_ok();
            supervisor::record_preparation_failure(&self.store, &plan, cleanup_complete)?;
            return Err(release_error.unwrap_or(validation_error));
        }
        let launch = AuthorizedLaunch::new(permit, plan);
        supervisor::start_native(&self.store, launch, Box::new(session), sandbox_plan.digest())
    }
}

fn validate_native_binding<B: NativeSandboxBackend>(
    plan: &ExecutionPlan,
    sandbox_plan: &CheckedSandboxPlan,
    admission: &BackendAdmission,
    backend: &B,
) -> Result<(), ProcessError> {
    let descriptor = backend.descriptor();
    let selected = plan.backend();
    let exact = plan.isolation() == crate::ExecutionIsolation::Restricted
        && descriptor.kind() == BackendKind::Native
        && backend.platform() == NativePlatform::current()
        && sandbox_plan.digest() == plan.sandbox_digest()
        && admission.plan_digest() == sandbox_plan.digest()
        && admission.descriptor() == descriptor
        && selected.name() == descriptor.name().as_str()
        && selected.version() == descriptor.version().as_str()
        && selected.descriptor_digest() == descriptor.digest()
        && selected.support_digest() == descriptor.support_digest()
        && selected.preparation_digest() == admission.preparation_digest();
    if !exact {
        return Err(ProcessError::new(
            ErrorCode::PlanMismatch,
            ProcessOperation::Authorize,
            RecoveryClass::SelectBackend,
            "native backend, platform, sandbox plan, or admission differs from execution",
        ));
    }
    Ok(())
}

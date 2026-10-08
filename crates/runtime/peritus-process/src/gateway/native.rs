//! Authorized restricted execution through one exact native backend.

use peritus_sandbox::{BackendAdmission, BackendKind, CheckedSandboxPlan};

use super::{AuthorizedLaunch, ExecutionGateway, ExecutionPermit, validate_request};
use crate::{
    AuthorizedPreparationContext, CancellationReason, ErrorCode, ExecutionAuthorizationRequest,
    ExecutionPlan, NativePlatform, NativeSandboxBackend, NativeSandboxSession, OwnedProcess,
    ProcessError, ProcessOperation, RecoveryClass, RetainedOwnerBinding, RetainedOwnerNonce,
    RetainedOwnerRequest, RetainedProcessKey, TerminalResult, supervisor,
};

impl ExecutionGateway {
    /// Reattaches the exact already-consumed native execution to its independent service owner.
    ///
    /// The request is loaded only from the claim-linked protected registry. This method never
    /// accepts caller-supplied plan bytes and never dispatches through the daemon process itself.
    ///
    /// # Errors
    /// Returns a typed recovery failure when no retained transport exists or any request, owner,
    /// operation, or service-generation binding differs.
    pub fn reattach_retained(
        &self,
        process_id: peritus_types::ProcessId,
    ) -> Result<OwnedProcess, ProcessError> {
        let transport = self.retained_owner.as_ref().ok_or_else(|| {
            retained_owner_error(
                ErrorCode::Unsupported,
                RecoveryClass::ReopenAndReconcile,
                "retained process owner transport is unavailable",
            )
        })?;
        let reservation = self
            .store
            .retained_owner_reservation(process_id)?
            .ok_or_else(|| {
                retained_owner_error(
                    ErrorCode::CorruptRecovery,
                    RecoveryClass::Quarantine,
                    "retained owner consumption claim is missing",
                )
            })?;
        let mut request_bytes = Vec::new();
        request_bytes
            .try_reserve_exact(reservation.request().len())
            .map_err(|_| {
                retained_owner_error(
                    ErrorCode::InvalidInput,
                    RecoveryClass::ReopenAndReconcile,
                    "retained owner request cannot be allocated",
                )
            })?;
        request_bytes.extend_from_slice(reservation.request());
        let retained = RetainedOwnerRequest::decode(request_bytes)?;
        let binding = retained.binding();
        let key = RetainedProcessKey::from_binding(binding);
        if binding.process_id() != process_id
            || binding.operation_digest() != reservation.operation_digest()
            || retained.digest() != reservation.request_digest()
            || binding.service_owner() != transport.service_owner()
        {
            return Err(retained_owner_error(
                ErrorCode::AuthorizationMismatch,
                RecoveryClass::Quarantine,
                "retained owner request differs from its service generation",
            ));
        }
        transport.launch_or_attach(key, retained.encode(), retained.digest())?;
        let observation = transport.observe(
            key,
            crate::ProcessCursor::after(0),
            0,
            None,
        )?;
        if retained.backend_factory_request().platform() == NativePlatform::Macos
            && !observation.matches_native_adoption(
                retained.backend_factory_request().platform(),
                binding,
            )
        {
            return Err(retained_owner_error(
                ErrorCode::Indeterminate,
                RecoveryClass::ReopenAndReconcile,
                "retained native session custody is unavailable or differs",
            ));
        }
        supervisor::attach_retained(
            &self.store,
            std::sync::Arc::clone(transport),
            key,
            retained.execution_plan().terminal_capabilities(),
        )
    }

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
        if let Some(transport) = &self.retained_owner {
            let factory = backend.retained_factory_request(sandbox_plan, admission)?;
            let transaction = self
                .store
                .retained_owner_transaction(plan.identity().process_id())?;
            if let Some(reservation) = self
                .store
                .retained_owner_reservation_in_transaction(&transaction)?
            {
                let mut request_bytes = Vec::new();
                request_bytes
                    .try_reserve_exact(reservation.request().len())
                    .map_err(|_| {
                        retained_owner_error(
                            ErrorCode::InvalidInput,
                            RecoveryClass::ReopenAndReconcile,
                            "retained owner request cannot be allocated",
                        )
                    })?;
                request_bytes.extend_from_slice(reservation.request());
                let retained = RetainedOwnerRequest::decode(request_bytes)?;
                let binding = retained.binding();
                let exact = binding.process_id() == plan.identity().process_id()
                    && binding.action_digest() == validation.action_digest
                    && binding.operation_digest() == reservation.operation_digest()
                    && binding.service_owner() == transport.service_owner()
                    && retained.digest() == reservation.request_digest()
                    && retained.execution_plan().canonical_bytes() == plan.canonical_bytes()
                    && retained.sandbox_plan().canonical_bytes()
                        == sandbox_plan.canonical_bytes()
                    && retained.backend_factory_request() == &factory;
                if !exact {
                    return Err(retained_owner_error(
                        ErrorCode::AuthorizationMismatch,
                        RecoveryClass::Quarantine,
                        "retained owner replay differs from its consumed native execution",
                    ));
                }
                let key = RetainedProcessKey::from_binding(binding);
                drop(transaction);
                transport.launch_or_attach(key, retained.encode(), retained.digest())?;
                if retained.backend_factory_request().platform() == NativePlatform::Macos {
                    let observation = transport.observe(
                        key,
                        crate::ProcessCursor::after(0),
                        0,
                        None,
                    )?;
                    if !observation.matches_native_adoption(
                        retained.backend_factory_request().platform(),
                        binding,
                    ) {
                        return Err(retained_owner_error(
                            ErrorCode::Indeterminate,
                            RecoveryClass::ReopenAndReconcile,
                            "retained native session custody is unavailable or differs",
                        ));
                    }
                }
                return supervisor::attach_retained(
                    &self.store,
                    std::sync::Arc::clone(transport),
                    key,
                    plan.terminal_capabilities(),
                );
            }
            let retained = match self
                .store
                .unclaimed_retained_owner_request(
                    &transaction,
                    &plan,
                    validation.action_digest,
                    validation.lease_claim,
                )?
            {
                Some(bytes) => {
                    let retained = RetainedOwnerRequest::decode(bytes)?;
                    let binding = retained.binding();
                    let exact = binding.process_id() == plan.identity().process_id()
                        && binding.action_digest() == validation.action_digest
                        && binding.service_owner() == transport.service_owner()
                        && retained.execution_plan().canonical_bytes() == plan.canonical_bytes()
                        && retained.sandbox_plan().canonical_bytes()
                            == sandbox_plan.canonical_bytes()
                        && retained.backend_factory_request() == &factory;
                    if !exact {
                        self.store
                            .discard_unclaimed_retained_owner_request(&transaction)?;
                        None
                    } else {
                        Some(retained)
                    }
                }
                None => None,
            };
            let retained = match retained {
                Some(retained) => retained,
                None => {
                    let binding = RetainedOwnerBinding::new(
                        plan.identity().process_id(),
                        RetainedOwnerNonce::allocate()?,
                        transport.service_owner(),
                        validation.action_digest,
                        plan.digest(),
                        sandbox_plan.digest(),
                        plan.backend().descriptor_digest(),
                        plan.backend().support_digest(),
                        plan.backend().preparation_digest(),
                    );
                    RetainedOwnerRequest::new(binding, &plan, sandbox_plan, factory)?
                }
            };
            let binding = retained.binding();
            backend.validate_preparation_capacity(sandbox_plan)?;
            self.store.consume_retained(
                &transaction,
                &plan,
                validation.action_digest,
                validation.lease_claim,
                binding,
                retained.encode(),
                retained.digest(),
            )?;
            drop(transaction);
            let key = RetainedProcessKey::from_binding(binding);
            // From this point the exact request is durably consumed and owned. A lost response is
            // an attachment failure, not permission to redispatch. The transport retains the
            // request locally and retries the same key on subsequent observation.
            let _ = transport.launch_or_attach(key, retained.encode(), retained.digest());
            return supervisor::attach_retained(
                &self.store,
                std::sync::Arc::clone(transport),
                key,
                plan.terminal_capabilities(),
            );
        }
        backend.validate_preparation_capacity(sandbox_plan)?;
        self.store.consume(&plan, validation.action_digest, validation.lease_claim)?;
        let context = AuthorizedPreparationContext::new(&plan, sandbox_plan, admission);
        let mut session = match backend.prepare(context) {
            Ok(session) => session,
            Err(error) => {
                let cleanup_complete = error.preparation_cleanup_complete().unwrap_or(false);
                supervisor::record_preparation_failure(&self.store, &plan, cleanup_complete)?;
                return Err(error);
            }
        };
        if let Err(validation_error) =
            crate::native::capture_prepared_session(
                &self.store,
                &mut session,
                &plan,
                sandbox_plan,
            )
        {
            let release_error = session.release().err();
            let release_capture = crate::native::capture_released_pre_start_session(
                &self.store,
                &mut session,
                &plan,
                sandbox_plan.digest(),
            );
            let cleanup_complete = release_error.is_none() && release_capture.is_ok();
            supervisor::record_preparation_failure(&self.store, &plan, cleanup_complete)?;
            return Err(release_error.unwrap_or(validation_error));
        }
        let launch = AuthorizedLaunch::new(permit, plan);
        supervisor::start_native(&self.store, launch, Box::new(session), sandbox_plan.digest())
    }

    /// Reconstructs the independently retained owner after verifying its already-consumed exact
    /// request against the protected registry.
    ///
    /// This entry point is reserved for the authenticated service supervisor. It accepts only an
    /// `Authorized` manifest. `Starting` is the durable launch-intended boundary and is never
    /// redispatched after owner loss.
    ///
    /// # Errors
    /// Returns a typed corruption, indeterminate-recovery, backend, preparation, or launch failure.
    pub fn resume_retained_with_backend<B>(
        &self,
        retained: &RetainedOwnerRequest,
        admission: &BackendAdmission,
        backend: B,
    ) -> Result<OwnedProcess, ProcessError>
    where
        B: NativeSandboxBackend,
    {
        self.resume_retained_with_backend_cancellable(
            retained,
            admission,
            backend,
            || None,
            || Ok(()),
        )
    }

    /// Reconstructs a retained owner while preserving cancellation through preparation and the
    /// exact native-dispatch boundary.
    ///
    /// `cancellation_requested` reports the first admitted reason. `enter_dispatch` is invoked
    /// only after backend preparation and returns a guard retained until the local owner has been
    /// created. This lets the service owner fence shutdown around the short dispatch operation
    /// without holding that fence across host probes or preparation.
    ///
    /// # Errors
    /// Returns a typed binding, cancellation, preparation, dispatch, or launch failure. A
    /// cancellation observed before dispatch is persisted as the unique terminal result.
    pub fn resume_retained_with_backend_cancellable<B, C, D, G>(
        &self,
        retained: &RetainedOwnerRequest,
        admission: &BackendAdmission,
        backend: B,
        cancellation_requested: C,
        enter_dispatch: D,
    ) -> Result<OwnedProcess, ProcessError>
    where
        B: NativeSandboxBackend,
        C: Fn() -> Option<CancellationReason>,
        D: FnOnce() -> Result<G, ProcessError>,
    {
        let binding = retained.binding();
        let process_id = binding.process_id();
        self.validate_retained_resume(retained)?;
        let plan = retained.execution_plan().clone();
        let sandbox = retained.sandbox_plan();
        validate_native_binding(&plan, sandbox, admission, &backend)?;
        supervisor::validate_native_launch(&plan)?;
        backend.validate_preparation_capacity(sandbox)?;
        if let Some(reason) = cancellation_requested() {
            // Backend construction may probe support, but the backend owns no prepared session or
            // session support tasks. Drop its inert configuration before certifying that absence.
            drop(backend);
            supervisor::record_preparation_cancellation(&self.store, &plan, reason, true)?;
            return Err(retained_launch_cancelled());
        }
        let context = AuthorizedPreparationContext::retained(&plan, sandbox, admission, binding);
        let mut session = match backend.prepare(context) {
            Ok(session) => session,
            Err(error) => {
                if let Some(reason) = cancellation_requested() {
                    let cleanup_complete =
                        error.preparation_cleanup_complete().unwrap_or(false);
                    supervisor::record_preparation_cancellation(
                        &self.store,
                        &plan,
                        reason,
                        cleanup_complete,
                    )?;
                } else {
                    let cleanup_complete =
                        error.preparation_cleanup_complete().unwrap_or(false);
                    supervisor::record_preparation_failure(
                        &self.store,
                        &plan,
                        cleanup_complete,
                    )?;
                }
                return Err(error);
            }
        };
        if let Err(validation_error) =
            crate::native::capture_prepared_session(&self.store, &mut session, &plan, sandbox)
        {
            let release_error = session.release().err();
            let release_capture = crate::native::capture_released_pre_start_session(
                &self.store,
                &mut session,
                &plan,
                sandbox.digest(),
            );
            let cleanup_complete = release_error.is_none() && release_capture.is_ok();
            if let Some(reason) = cancellation_requested() {
                supervisor::record_preparation_cancellation(
                    &self.store,
                    &plan,
                    reason,
                    cleanup_complete,
                )?;
            } else {
                supervisor::record_preparation_failure(&self.store, &plan, cleanup_complete)?;
            }
            return Err(release_error.unwrap_or(validation_error));
        }
        if let Some(reason) = cancellation_requested() {
            release_retained_before_start(
                &self.store,
                &mut session,
                &plan,
                sandbox.digest(),
                reason,
            )?;
            return Err(retained_launch_cancelled());
        }
        let _dispatch = match enter_dispatch() {
            Ok(guard) => guard,
            Err(error) => {
                if let Some(reason) = cancellation_requested() {
                    release_retained_before_start(
                        &self.store,
                        &mut session,
                        &plan,
                        sandbox.digest(),
                        reason,
                    )?;
                } else {
                    let release_error = session.release().err();
                    let release_capture = crate::native::capture_released_pre_start_session(
                        &self.store,
                        &mut session,
                        &plan,
                        sandbox.digest(),
                    );
                    let cleanup_complete = release_error.is_none() && release_capture.is_ok();
                    supervisor::record_preparation_failure(
                        &self.store,
                        &plan,
                        cleanup_complete,
                    )?;
                }
                return Err(error);
            }
        };
        if let Some(reason) = cancellation_requested() {
            release_retained_before_start(
                &self.store,
                &mut session,
                &plan,
                sandbox.digest(),
                reason,
            )?;
            return Err(retained_launch_cancelled());
        }
        let permit = ExecutionPermit {
            _action_id: plan.identity().action_id(),
            _process_id: process_id,
            action_digest: binding.action_digest(),
            _plan_digest: plan.digest(),
        };
        let launch = AuthorizedLaunch::new(permit, plan);
        supervisor::start_native(&self.store, launch, Box::new(session), sandbox.digest())
    }

    /// Persists a cancellation that won before a retained backend could be reconstructed.
    ///
    /// # Errors
    /// Returns a typed binding or persistence failure.
    pub fn cancel_retained_before_preparation(
        &self,
        retained: &RetainedOwnerRequest,
        reason: CancellationReason,
    ) -> Result<TerminalResult, ProcessError> {
        self.validate_retained_resume(retained)?;
        supervisor::record_preparation_cancellation(
            &self.store,
            retained.execution_plan(),
            reason,
            true,
        )
    }

    fn validate_retained_resume(
        &self,
        retained: &RetainedOwnerRequest,
    ) -> Result<(), ProcessError> {
        let binding = retained.binding();
        let reservation = self
            .store
            .retained_owner_reservation(binding.process_id())?
            .ok_or_else(|| retained_owner_error(
                ErrorCode::CorruptRecovery,
                RecoveryClass::Quarantine,
                "retained owner consumption claim is missing",
            ))?;
        if reservation.operation_digest() != binding.operation_digest()
            || reservation.request_digest() != retained.digest()
            || reservation.request() != retained.encode()
        {
            return Err(retained_owner_error(
                ErrorCode::CorruptRecovery,
                RecoveryClass::Quarantine,
                "retained owner request differs from its consumed registry record",
            ));
        }
        if reservation.phase() != crate::LifecyclePhase::Authorized {
            return Err(retained_owner_error(
                ErrorCode::Indeterminate,
                RecoveryClass::ReopenAndReconcile,
                "retained owner launch was already intended and cannot be redispatched",
            ));
        }
        Ok(())
    }
}

fn release_retained_before_start<S: NativeSandboxSession>(
    store: &crate::ProcessStore,
    session: &mut S,
    plan: &ExecutionPlan,
    sandbox_digest: peritus_types::Sha256Digest,
    reason: CancellationReason,
) -> Result<(), ProcessError> {
    let release_error = session.release().err();
    let release_capture = crate::native::capture_released_pre_start_session(
        store,
        session,
        plan,
        sandbox_digest,
    );
    let cleanup_complete = release_error.is_none() && release_capture.is_ok();
    supervisor::record_preparation_cancellation(store, plan, reason, cleanup_complete)?;
    release_error.map_or(Ok(()), Err)
}

const fn retained_launch_cancelled() -> ProcessError {
    retained_owner_error(
        ErrorCode::Supervisor,
        RecoveryClass::Terminal,
        "retained launch was cancelled before native dispatch",
    )
}

const fn retained_owner_error(
    code: ErrorCode,
    recovery: RecoveryClass,
    detail: &'static str,
) -> ProcessError {
    ProcessError::new(code, ProcessOperation::Reconcile, recovery, detail)
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

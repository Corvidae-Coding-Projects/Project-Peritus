use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

#[cfg(unix)]
use peritus_process::TerminalSize;
use peritus_process::{
    AuthorizedPreparationContext, CancellationReason, CommandSpec, ErrorCode,
    ExecutionAuthorizationRequest, ExecutionGateway, GracefulAction, IoMode,
    NativeLaunchDescription, NativePlatform, NativePoll, NativeProtectedHandle,
    NativeSandboxBackend, NativeSandboxSession, OsExitObservation, OwnedProcess, ProcessError,
    ProcessOperation, ProcessStore, ProcessTreeIdentity, RecoveryClass, StdinPolicy,
    TerminalDisposition,
};
use peritus_sandbox::{
    BackendAdmission, BackendDescriptor, BackendKind, BackendName, BackendVersion,
    CapabilityDomain, EnforcementObservation, FeatureSet, ObservationDisposition, ObservationKind,
    PathSemantics, ResourceFidelity,
};

use crate::support::{
    Ids, PlanOptions, TestRoot, commit_authority, intent, native_helper_binary, native_plan,
    open_journal,
};

#[derive(Clone)]
struct LifecycleProbe {
    prepare_calls: Arc<AtomicUsize>,
    events: Arc<Mutex<Vec<ObservationKind>>>,
}

impl LifecycleProbe {
    fn new() -> Self {
        Self {
            prepare_calls: Arc::new(AtomicUsize::new(0)),
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn prepare_calls(&self) -> usize {
        self.prepare_calls.load(Ordering::SeqCst)
    }

    fn events(&self) -> Vec<ObservationKind> {
        self.events.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }
}

#[allow(
    clippy::struct_excessive_bools,
    reason = "independent injected native fault switches remain explicit in the test fixture"
)]
struct TestBackend {
    descriptor: BackendDescriptor,
    probe: LifecycleProbe,
    helper: String,
    fail_prepare: bool,
    fail_release: bool,
    invalidate_prepared_observation: bool,
    limit_on_poll: bool,
    protected_payload: bool,
}

impl TestBackend {
    fn admitted(admission: &BackendAdmission, probe: LifecycleProbe) -> Self {
        Self {
            descriptor: admission.descriptor().clone(),
            probe,
            helper: native_helper_binary(),
            fail_prepare: false,
            fail_release: false,
            invalidate_prepared_observation: false,
            limit_on_poll: false,
            protected_payload: false,
        }
    }
}

impl NativeSandboxBackend for TestBackend {
    type Session = TestSession;

    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn platform(&self) -> NativePlatform {
        NativePlatform::current()
    }

    fn prepare(
        self,
        context: AuthorizedPreparationContext<'_>,
    ) -> Result<Self::Session, ProcessError> {
        self.probe.prepare_calls.fetch_add(1, Ordering::SeqCst);
        if self.fail_prepare {
            return Err(ProcessError::new(
                ErrorCode::Unsupported,
                ProcessOperation::Spawn,
                RecoveryClass::SelectBackend,
                "injected native preparation failure",
            ));
        }
        let execution = context.execution_plan();
        let protected = self
            .protected_payload
            .then(|| {
                NativeProtectedHandle::from_bytes(
                    "test-secret-environment",
                    b"peritus-protected-test-payload".to_vec(),
                )
            })
            .transpose()?;
        let mut manifest = Vec::from(context.admission().preparation_digest().as_bytes());
        if let Some(handle) = &protected {
            manifest.extend_from_slice(b"peritus-native-protected-test-v1\0");
            manifest.extend_from_slice(&handle.raw_handle().to_le_bytes());
        } else {
            manifest.extend_from_slice(b"peritus-native-test-manifest-v1");
        }
        let digest = peritus_codec::sha256(&manifest);
        let arguments = std::iter::once(execution.command().executable().to_owned())
            .chain(execution.command().arguments().iter().cloned());
        let command = CommandSpec::new(self.helper, arguments)?;
        let launch = NativeLaunchDescription::new(
            command,
            "peritus-native-helper-fixture-v1",
            manifest,
            digest,
            context.admission().preparation_digest(),
        )?
        .with_protected_handles(protected.into_iter().collect())?;
        let plan_digest = context.sandbox_plan().digest();
        let backend_digest = if self.invalidate_prepared_observation {
            peritus_codec::sha256(b"invalid-prepared-backend-binding")
        } else {
            context.admission().descriptor_digest()
        };
        self.probe
            .events
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(ObservationKind::Prepared);
        Ok(TestSession {
            launch,
            plan_digest,
            backend_digest,
            observations: vec![observation(
                1,
                plan_digest,
                backend_digest,
                ObservationKind::Prepared,
            )],
            probe: self.probe,
            fail_release: self.fail_release,
            limit_on_poll: self.limit_on_poll,
        })
    }
}

struct TestSession {
    launch: NativeLaunchDescription,
    plan_digest: peritus_types::Sha256Digest,
    backend_digest: peritus_types::Sha256Digest,
    observations: Vec<EnforcementObservation>,
    probe: LifecycleProbe,
    fail_release: bool,
    limit_on_poll: bool,
}

impl TestSession {
    fn record(&mut self, kind: ObservationKind) {
        let sequence = u64::try_from(self.observations.len()).unwrap_or(u64::MAX).saturating_add(1);
        self.observations.push(observation(sequence, self.plan_digest, self.backend_digest, kind));
        self.probe.events.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(kind);
    }
}

impl NativeSandboxSession for TestSession {
    fn launch_description(&self) -> &NativeLaunchDescription {
        &self.launch
    }

    fn observations(&self) -> &[EnforcementObservation] {
        &self.observations
    }

    fn poll_resources(&mut self, _tree: ProcessTreeIdentity) -> Result<NativePoll, ProcessError> {
        if self.limit_on_poll {
            self.limit_on_poll = false;
            self.record(ObservationKind::ResourceCharged);
            Ok(NativePoll::ResourceLimitExceeded)
        } else {
            Ok(NativePoll::Continue)
        }
    }

    fn activated(&mut self, _tree: ProcessTreeIdentity) -> Result<(), ProcessError> {
        self.record(ObservationKind::Activated);
        Ok(())
    }

    fn cancellation_requested(&mut self, _reason: CancellationReason) -> Result<(), ProcessError> {
        self.record(ObservationKind::Cancellation);
        Ok(())
    }

    fn terminated(&mut self, _exit: &OsExitObservation) -> Result<(), ProcessError> {
        self.record(ObservationKind::Terminated);
        Ok(())
    }

    fn release(&mut self) -> Result<(), ProcessError> {
        if self.fail_release {
            return Err(ProcessError::new(
                ErrorCode::Supervisor,
                ProcessOperation::Wait,
                RecoveryClass::Quarantine,
                "injected native release failure",
            ));
        }
        self.record(ObservationKind::Released);
        Ok(())
    }
}

const fn observation(
    sequence: u64,
    plan_digest: peritus_types::Sha256Digest,
    backend_digest: peritus_types::Sha256Digest,
    kind: ObservationKind,
) -> EnforcementObservation {
    EnforcementObservation::new(
        sequence,
        plan_digest,
        backend_digest,
        kind,
        Some(CapabilityDomain::Process),
        ObservationDisposition::Completed,
    )
}

const fn options(arguments: Vec<String>, stdin: StdinPolicy) -> PlanOptions<'static> {
    PlanOptions {
        arguments,
        environment: Vec::new(),
        io: IoMode::Pipes,
        stdin,
        output_limit: 64,
        wall_timeout: None,
        graceful: GracefulAction::Terminate,
        grace_millis: 100,
        process_count: 1,
        descendants: 0,
        workspace_access: peritus_process::WorkspaceAccess::ReadOnly,
        resize_allowed: true,
        environment_authority: None,
        resource_fidelity: ResourceFidelity::Hard,
    }
}

#[path = "native_backend/lifecycle.rs"]
mod lifecycle;
#[path = "native_backend/rejection.rs"]
mod rejection;

const fn request<'a>(
    ids: &Ids,
    execution: &peritus_process::ExecutionPlan,
    action: &'a peritus_protocol::ActionIntentDto,
    receipts: &'a crate::support::authority::AuthorityReceipts,
) -> ExecutionAuthorizationRequest<'a> {
    ExecutionAuthorizationRequest::new(
        action,
        &receipts.kernel,
        &receipts.capability,
        &receipts.budget,
        None,
        &receipts.epoch,
        ids.revision,
        ids.session,
        ids.revision.workspace_generation(),
        ids.revision.workspace_revision(),
        receipts.observed_at,
        execution.digest(),
    )
}

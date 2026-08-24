//! Native C3 fixture and exact sandbox projection for C4 integration tests.

mod plan;
mod sandbox;

pub use plan::{quality_plan, shell_plan};
pub use sandbox::sandbox;

use peritus_process::{
    AuthorizedPreparationContext, CancellationReason, CommandSpec, NativeLaunchDescription,
    NativePlatform, NativeSandboxBackend, NativeSandboxSession, OsExitObservation, ProcessError,
    ProcessTreeIdentity,
};
use peritus_sandbox::{
    BackendAdmission, BackendDescriptor, CapabilityDomain, EnforcementObservation,
    ObservationDisposition, ObservationKind,
};
use peritus_types::Sha256Digest;

pub struct NativeBackend {
    descriptor: BackendDescriptor,
    helper: String,
}

impl NativeBackend {
    pub fn admitted(admission: &BackendAdmission) -> Self {
        Self {
            descriptor: admission.descriptor().clone(),
            helper: std::env::var("CARGO_BIN_EXE_peritus-c4-native-helper-fixture")
                .expect("Cargo native helper fixture path"),
        }
    }
}

impl NativeSandboxBackend for NativeBackend {
    type Session = NativeSession;

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
        let execution = context.execution_plan();
        let manifest = context.admission().preparation_digest().as_bytes().to_vec();
        let manifest_digest = peritus_codec::sha256(&manifest);
        let arguments = std::iter::once(execution.command().executable().to_owned())
            .chain(execution.command().arguments().iter().cloned());
        let command = CommandSpec::new(self.helper, arguments)?;
        let launch = NativeLaunchDescription::new(
            command,
            "peritus-c4-native-helper-fixture-v1",
            manifest,
            manifest_digest,
            context.admission().preparation_digest(),
        )?;
        let plan_digest = context.sandbox_plan().digest();
        let backend_digest = context.admission().descriptor_digest();
        Ok(NativeSession {
            launch,
            plan_digest,
            backend_digest,
            observations: vec![observation(
                1,
                plan_digest,
                backend_digest,
                ObservationKind::Prepared,
            )],
        })
    }
}

pub struct NativeSession {
    launch: NativeLaunchDescription,
    plan_digest: Sha256Digest,
    backend_digest: Sha256Digest,
    observations: Vec<EnforcementObservation>,
}

impl NativeSession {
    fn record(&mut self, kind: ObservationKind) {
        let sequence = u64::try_from(self.observations.len()).unwrap_or(u64::MAX).saturating_add(1);
        self.observations.push(observation(sequence, self.plan_digest, self.backend_digest, kind));
    }
}

impl NativeSandboxSession for NativeSession {
    fn launch_description(&self) -> &NativeLaunchDescription {
        &self.launch
    }

    fn observations(&self) -> &[EnforcementObservation] {
        &self.observations
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
        self.record(ObservationKind::Released);
        Ok(())
    }
}

const fn observation(
    sequence: u64,
    plan_digest: Sha256Digest,
    backend_digest: Sha256Digest,
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

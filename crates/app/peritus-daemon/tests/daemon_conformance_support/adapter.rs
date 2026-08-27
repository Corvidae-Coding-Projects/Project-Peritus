//! Production subprocess adapter and explicit public-seam coverage inventory.

use std::io::Write;

use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, DaemonConformanceError, DaemonConformanceFixture,
    DaemonConformanceObservation, DaemonConformanceSubject, DaemonScenario, SubjectDescriptor,
    SubjectFactory, SubjectFailure,
};

use super::{artifact, lifecycle, outbox, prompt, session, subscription, terminal};

const REACHABLE: &[DaemonScenario] = &[
    DaemonScenario::CompatibleSession,
    DaemonScenario::IncompatibleSession,
    DaemonScenario::PeerActorMismatch,
    DaemonScenario::ContextMismatch,
    DaemonScenario::NewCommand,
    DaemonScenario::ReplayCommand,
    DaemonScenario::ConflictingCommand,
    DaemonScenario::IndeterminateCommand,
    DaemonScenario::StaleRevision,
    DaemonScenario::SubscriptionResume,
    DaemonScenario::SubscriptionRedelivery,
    DaemonScenario::SubscriptionAcknowledgement,
    DaemonScenario::SubscriptionGap,
    DaemonScenario::SubscriptionBackpressure,
    DaemonScenario::ArtifactDownload,
    DaemonScenario::ArtifactUpload,
    DaemonScenario::ArtifactCorruption,
    DaemonScenario::SecondInstance,
    DaemonScenario::GracefulShutdown,
    DaemonScenario::ForcedRestart,
    DaemonScenario::Bounds,
    DaemonScenario::MalformedFrame,
    DaemonScenario::NonAuthority,
    DaemonScenario::ReadOnlyAdmission,
    DaemonScenario::StartupFailure,
    DaemonScenario::PromptFreshness,
    DaemonScenario::PtyOrdering,
    DaemonScenario::OutboxCrash,
];

/// Returns scenarios the current public binary can exercise without internal imports.
pub const fn reachable_scenarios() -> &'static [DaemonScenario] {
    REACHABLE
}

/// Returns the exact missing public production seam for a currently unreachable scenario.
pub const fn blocker_for(scenario: DaemonScenario) -> Option<&'static str> {
    let _ = scenario;
    None
}

pub struct BinaryDaemonFactory {
    descriptor: SubjectDescriptor,
}

impl BinaryDaemonFactory {
    pub(crate) fn new() -> Self {
        Self {
            descriptor: SubjectDescriptor::new(
                peritus_conformance::ReportText::new("peritusd-production-process")
                    .expect("static subject name is valid report text"),
                peritus_conformance::ReportText::new("public A3 subprocess adapter")
                    .expect("static implementation name is valid report text"),
            ),
        }
    }
}

pub struct BinaryDaemonSubject;

impl DaemonConformanceSubject for BinaryDaemonSubject {
    fn exercise(
        &mut self,
        fixture: &DaemonConformanceFixture,
    ) -> Result<DaemonConformanceObservation, DaemonConformanceError> {
        let result = match fixture.scenario() {
            DaemonScenario::CompatibleSession => session::compatible_session(),
            DaemonScenario::IncompatibleSession => session::incompatible_session(),
            DaemonScenario::PeerActorMismatch => session::peer_actor_mismatch(),
            DaemonScenario::ContextMismatch => session::context_mismatch(),
            DaemonScenario::NewCommand => session::new_command(),
            DaemonScenario::ReplayCommand => session::replay_command(),
            DaemonScenario::ConflictingCommand => session::conflicting_command(),
            DaemonScenario::IndeterminateCommand => session::indeterminate_command(),
            DaemonScenario::StaleRevision => session::stale_revision(),
            DaemonScenario::SubscriptionResume => subscription::resume(fixture),
            DaemonScenario::SubscriptionRedelivery => subscription::redelivery(fixture),
            DaemonScenario::SubscriptionAcknowledgement => subscription::acknowledgement(fixture),
            DaemonScenario::SubscriptionGap => subscription::gap(),
            DaemonScenario::SubscriptionBackpressure => subscription::backpressure(fixture),
            DaemonScenario::ArtifactDownload => artifact::download(fixture),
            DaemonScenario::ArtifactUpload => artifact::upload(fixture),
            DaemonScenario::ArtifactCorruption => artifact::corruption(fixture),
            DaemonScenario::SecondInstance => lifecycle::second_instance(),
            DaemonScenario::GracefulShutdown => lifecycle::graceful_shutdown(),
            DaemonScenario::ForcedRestart => lifecycle::forced_restart(),
            DaemonScenario::Bounds => lifecycle::bounds(fixture),
            DaemonScenario::MalformedFrame => lifecycle::malformed_frame(),
            DaemonScenario::NonAuthority => lifecycle::non_authority(),
            DaemonScenario::ReadOnlyAdmission => lifecycle::read_only_admission(),
            DaemonScenario::StartupFailure => lifecycle::startup_failure(),
            DaemonScenario::PromptFreshness => prompt::freshness(),
            DaemonScenario::PtyOrdering => terminal::pty_ordering(),
            DaemonScenario::OutboxCrash => outbox::crash_recovery(),
        };
        result.map_err(|error| {
            let message = format!("peritusd {:?} exercise failed: {error}\n", fixture.scenario());
            let _ = std::io::stderr().lock().write_all(message.as_bytes());
            DaemonConformanceError::Transport
        })
    }
}

impl SubjectFactory<BinaryDaemonSubject> for BinaryDaemonFactory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<BinaryDaemonSubject, SubjectFailure>> {
        Box::pin(async { Ok(BinaryDaemonSubject) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: BinaryDaemonSubject,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

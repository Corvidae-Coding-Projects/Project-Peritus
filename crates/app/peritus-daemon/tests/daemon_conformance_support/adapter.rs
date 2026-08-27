//! Production subprocess adapter and explicit public-seam coverage inventory.

use std::io::Write;

use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, DaemonConformanceError, DaemonConformanceFixture,
    DaemonConformanceObservation, DaemonConformanceSubject, DaemonScenario, SubjectDescriptor,
    SubjectFactory, SubjectFailure,
};

use super::{artifact, lifecycle, session, subscription};

const REACHABLE: &[DaemonScenario] = &[
    DaemonScenario::CompatibleSession,
    DaemonScenario::IncompatibleSession,
    DaemonScenario::PeerActorMismatch,
    DaemonScenario::ContextMismatch,
    DaemonScenario::NewCommand,
    DaemonScenario::ReplayCommand,
    DaemonScenario::ConflictingCommand,
    DaemonScenario::StaleRevision,
    DaemonScenario::SubscriptionResume,
    DaemonScenario::SubscriptionRedelivery,
    DaemonScenario::SubscriptionAcknowledgement,
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
];

/// Returns scenarios the current public binary can exercise without internal imports.
pub(crate) const fn reachable_scenarios() -> &'static [DaemonScenario] {
    REACHABLE
}

/// Returns the exact missing public production seam for a currently unreachable scenario.
pub(crate) const fn blocker_for(scenario: DaemonScenario) -> Option<&'static str> {
    Some(match scenario {
        DaemonScenario::IndeterminateCommand => {
            "peritusd exposes no public command checkpoint or failpoint that can leave and observe an indeterminate application command"
        }
        DaemonScenario::SubscriptionGap => {
            "peritusd exposes no public retention transition or fixture import that can place a cursor before retained global history"
        }
        DaemonScenario::PromptFreshness => {
            "peritusd exposes prompt answers but no public command that creates an outstanding actor-owned prompt challenge"
        }
        DaemonScenario::PtyOrdering => {
            "peritusd exposes terminal attachment but no public command that starts and registers a C2-owned PTY process"
        }
        DaemonScenario::ReadOnlyAdmission => {
            "peritusd exits on startup error and exposes no public transition that leaves IPC serving in ReadyReadOnly"
        }
        DaemonScenario::StartupFailure => {
            "peritusd has no public diagnostic-safe startup failpoint and exits instead of serving the required typed read-only report"
        }
        DaemonScenario::OutboxCrash => {
            "peritusd exposes no public effect-before-ack crash checkpoint or destination observation control"
        }
        _ => return None,
    })
}

pub(crate) struct BinaryDaemonFactory {
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

pub(crate) struct BinaryDaemonSubject;

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
            DaemonScenario::StaleRevision => session::stale_revision(),
            DaemonScenario::SubscriptionResume => subscription::resume(fixture),
            DaemonScenario::SubscriptionRedelivery => subscription::redelivery(fixture),
            DaemonScenario::SubscriptionAcknowledgement => subscription::acknowledgement(fixture),
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
            DaemonScenario::IndeterminateCommand
            | DaemonScenario::SubscriptionGap
            | DaemonScenario::PromptFreshness
            | DaemonScenario::PtyOrdering
            | DaemonScenario::ReadOnlyAdmission
            | DaemonScenario::StartupFailure
            | DaemonScenario::OutboxCrash => {
                return Err(DaemonConformanceError::Observation);
            }
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

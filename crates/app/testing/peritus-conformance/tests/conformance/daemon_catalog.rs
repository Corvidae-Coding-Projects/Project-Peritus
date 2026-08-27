use peritus_conformance::{
    CaseDescriptor, CaseStatus, ConformanceFuture, ConformanceRunner, DaemonAdmissionObservation,
    DaemonArtifactIntegrity, DaemonArtifactObservation, DaemonArtifactOutcome,
    DaemonArtifactPublication, DaemonBoundsObservation, DaemonCommandObservation,
    DaemonCommandOutcome, DaemonConformanceError, DaemonConformanceFixture,
    DaemonConformanceObservation, DaemonConformanceSubject, DaemonFrameObservation,
    DaemonInstanceObservation, DaemonNonAuthorityObservation, DaemonOutboxObservation,
    DaemonPromptObservation, DaemonPromptRejection, DaemonReadiness, DaemonRecoveryObservation,
    DaemonScenario, DaemonSessionObservation, DaemonSessionOutcome, DaemonShutdownObservation,
    DaemonShutdownOutcome, DaemonStartupObservation, DaemonSubscriptionObservation,
    DaemonSubscriptionOutcome, DaemonTerminalObservation, SubjectDescriptor, SubjectFactory,
    SubjectFailure, SuiteStatus, daemon_suite,
};

use super::harness::{block_on, text};

#[derive(Clone, Copy, Default)]
enum Behavior {
    #[default]
    Exact,
    AdmitReadOnlyMutation,
}

struct ReferenceDaemon {
    behavior: Behavior,
}

impl DaemonConformanceSubject for ReferenceDaemon {
    fn exercise(
        &mut self,
        fixture: &DaemonConformanceFixture,
    ) -> Result<DaemonConformanceObservation, DaemonConformanceError> {
        use DaemonConformanceObservation as O;
        use DaemonScenario as D;
        Ok(match fixture.scenario() {
            D::CompatibleSession => O::Session(DaemonSessionObservation::new(
                DaemonSessionOutcome::Established,
                true,
                true,
                true,
                1,
                0,
            )),
            D::IncompatibleSession => O::Session(DaemonSessionObservation::new(
                DaemonSessionOutcome::Incompatible,
                false,
                true,
                true,
                0,
                0,
            )),
            D::PeerActorMismatch => O::Session(DaemonSessionObservation::new(
                DaemonSessionOutcome::Rejected,
                false,
                false,
                true,
                0,
                0,
            )),
            D::ContextMismatch => O::Session(DaemonSessionObservation::new(
                DaemonSessionOutcome::Rejected,
                true,
                true,
                false,
                0,
                0,
            )),
            D::NewCommand => command(DaemonCommandOutcome::Committed, 2, true, false, 0, 2, 1),
            D::ReplayCommand => command(DaemonCommandOutcome::Replayed, 2, true, true, 0, 0, 0),
            D::ConflictingCommand => {
                command(DaemonCommandOutcome::Conflict, 0, false, false, 0, 0, 0)
            }
            D::IndeterminateCommand => {
                command(DaemonCommandOutcome::Indeterminate, 0, false, true, 0, 0, 0)
            }
            D::StaleRevision => command(DaemonCommandOutcome::Rejected, 0, false, false, 0, 0, 0),
            D::SubscriptionResume => subscription(
                DaemonSubscriptionOutcome::Active,
                fixture.source_cursor(),
                Some(fixture.source_cursor() + 2),
                0,
                true,
                false,
                false,
                0,
                0,
                1,
            ),
            D::SubscriptionRedelivery => subscription(
                DaemonSubscriptionOutcome::Active,
                fixture.source_cursor(),
                Some(fixture.source_cursor() + 1),
                1,
                true,
                true,
                false,
                0,
                0,
                1,
            ),
            D::SubscriptionAcknowledgement => subscription(
                DaemonSubscriptionOutcome::Acknowledged,
                fixture.source_cursor(),
                Some(fixture.source_cursor() + 1),
                0,
                true,
                true,
                true,
                1,
                0,
                1,
            ),
            D::SubscriptionGap => subscription(
                DaemonSubscriptionOutcome::SnapshotRequired,
                fixture.source_cursor(),
                None,
                0,
                false,
                false,
                false,
                0,
                0,
                0,
            ),
            D::SubscriptionBackpressure => subscription(
                DaemonSubscriptionOutcome::Backpressured,
                fixture.source_cursor(),
                Some(fixture.source_cursor() + 1),
                0,
                true,
                true,
                false,
                0,
                0,
                fixture.maximum_in_flight(),
            ),
            D::ArtifactDownload => artifact(
                DaemonArtifactOutcome::Downloaded,
                fixture.artifact_size(),
                DaemonArtifactIntegrity::Exact,
                DaemonArtifactPublication::Available,
            ),
            D::ArtifactUpload => artifact(
                DaemonArtifactOutcome::Uploaded,
                fixture.artifact_size(),
                DaemonArtifactIntegrity::Exact,
                DaemonArtifactPublication::Published,
            ),
            D::ArtifactCorruption => artifact(
                DaemonArtifactOutcome::CorruptRejected,
                fixture.artifact_size(),
                DaemonArtifactIntegrity::Mismatched,
                DaemonArtifactPublication::Withheld,
            ),
            D::PromptFreshness => O::Prompt(DaemonPromptObservation::new(
                true,
                vec![
                    DaemonPromptRejection::ActorSessionMismatch,
                    DaemonPromptRejection::StaleRevisionGeneration,
                    DaemonPromptRejection::UnsignedApproval,
                ],
                1,
            )),
            D::PtyOrdering => {
                O::Terminal(DaemonTerminalObservation::new(64, true, true, true, 1, 64, 128))
            }
            D::ReadOnlyAdmission => O::Admission(DaemonAdmissionObservation::new(
                DaemonReadiness::ReadyReadOnly,
                true,
                matches!(self.behavior, Behavior::AdmitReadOnlyMutation),
                0,
            )),
            D::SecondInstance => O::Instance(DaemonInstanceObservation::new(true, true, false, 0)),
            D::StartupFailure => O::Startup(DaemonStartupObservation::new(
                DaemonReadiness::ReadyReadOnly,
                true,
                0,
                0,
                true,
            )),
            D::OutboxCrash => O::Outbox(DaemonOutboxObservation::new(true, 1, 0, true, 0)),
            D::GracefulShutdown => O::Shutdown(DaemonShutdownObservation::new(
                DaemonShutdownOutcome::Clean,
                true,
                true,
                true,
                0,
            )),
            D::ForcedRestart => {
                O::Recovery(DaemonRecoveryObservation::new(true, true, 0, 0, false))
            }
            D::Bounds => O::Bounds(DaemonBoundsObservation::new(
                true,
                0,
                fixture.maximum_in_flight(),
                fixture.maximum_in_flight(),
            )),
            D::MalformedFrame => O::Frame(DaemonFrameObservation::new(true, 0, 0, 0)),
            D::NonAuthority => {
                O::NonAuthority(DaemonNonAuthorityObservation::new(true, 0, 0, false))
            }
        })
    }
}

fn command(
    outcome: DaemonCommandOutcome,
    events: u64,
    range_exact: bool,
    reconciled: bool,
    replacements: u64,
    appends: u64,
    effects: u64,
) -> DaemonConformanceObservation {
    DaemonConformanceObservation::Command(DaemonCommandObservation::new(
        outcome,
        events,
        range_exact,
        reconciled,
        replacements,
        appends,
        effects,
    ))
}

#[allow(clippy::too_many_arguments, reason = "mirrors the direct subscription observation")]
fn subscription(
    outcome: DaemonSubscriptionOutcome,
    supplied_cursor: u64,
    first_cursor: Option<u64>,
    redeliveries: u64,
    stable_event: bool,
    distinct_attempt: bool,
    ack_contiguous: bool,
    released: u64,
    deleted: u64,
    peak: u64,
) -> DaemonConformanceObservation {
    DaemonConformanceObservation::Subscription(DaemonSubscriptionObservation::new(
        outcome,
        supplied_cursor,
        first_cursor,
        redeliveries,
        stable_event,
        distinct_attempt,
        ack_contiguous,
        released,
        deleted,
        peak,
    ))
}

fn artifact(
    outcome: DaemonArtifactOutcome,
    bytes: u64,
    integrity: DaemonArtifactIntegrity,
    publication: DaemonArtifactPublication,
) -> DaemonConformanceObservation {
    DaemonConformanceObservation::Artifact(DaemonArtifactObservation::new(
        outcome,
        bytes,
        integrity,
        publication,
    ))
}

struct Factory {
    descriptor: SubjectDescriptor,
    behavior: Behavior,
}

impl Factory {
    fn new(behavior: Behavior) -> Self {
        Self {
            descriptor: SubjectDescriptor::new(text("daemon-reference"), text("A2 G0 oracle")),
            behavior,
        }
    }
}

impl SubjectFactory<ReferenceDaemon> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceDaemon, SubjectFailure>> {
        let behavior = self.behavior;
        Box::pin(async move { Ok(ReferenceDaemon { behavior }) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceDaemon,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn daemon_catalog_runs_all_twenty_eight_cases() {
    let report = block_on(ConformanceRunner::run(
        &daemon_suite::<ReferenceDaemon>(),
        &Factory::new(Behavior::Exact),
    ));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 28);
}

#[test]
fn daemon_catalog_rejects_read_only_mutation_admission() {
    let report = block_on(ConformanceRunner::run(
        &daemon_suite::<ReferenceDaemon>(),
        &Factory::new(Behavior::AdmitReadOnlyMutation),
    ));
    assert!(report.cases().iter().any(|case| {
        case.status() == CaseStatus::Failed
            && case.descriptor().id().as_str() == "peritus.daemon.read-only-admission"
    }));
}

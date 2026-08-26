use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, ConformanceSuite,
    ProtocolConformanceError, ProtocolConformanceFixture, ProtocolConformanceObservation,
    ProtocolConformanceSubject, ProtocolScenario, SubjectDescriptor, SubjectFactory,
    SubjectFailure, SuiteStatus, protocol_suite,
};

use super::harness::{TestSubject, block_on, text};

struct ReferenceProtocol;

impl ProtocolConformanceSubject for ReferenceProtocol {
    fn exercise(
        &mut self,
        fixture: &ProtocolConformanceFixture,
    ) -> Result<ProtocolConformanceObservation, ProtocolConformanceError> {
        use ProtocolScenario as P;
        let scenario = fixture.scenario();
        Ok(ProtocolConformanceObservation {
            expected_terminal: true,
            negotiation_exact: matches!(
                scenario,
                P::NegotiationExact
                    | P::NegotiationDowngraded
                    | P::NegotiationIncompatible
                    | P::RequiredFeature
            ),
            command_binding_exact: scenario == P::CommandBinding,
            idempotency_exact: scenario == P::Idempotency,
            delivery_exact: matches!(scenario, P::SubscriptionResume | P::AckLegality),
            flow_control_exact: matches!(scenario, P::GapSnapshot | P::Backpressure),
            artifact_exact: scenario == P::ArtifactTransfer,
            prompt_exact: scenario == P::PromptFreshness,
            terminal_exact: scenario == P::TerminalOrdering,
            daemon_control_exact: scenario == P::DaemonLifecycle,
            malformed_rejected: scenario == P::MalformedInput,
            bounds_enforced: scenario == P::Bounds,
            stable_error_exact: true,
            non_authoritative: true,
        })
    }
}

struct Factory {
    descriptor: SubjectDescriptor,
}

impl Factory {
    fn new() -> Self {
        Self {
            descriptor: SubjectDescriptor::new(text("protocol-reference"), text("A2 A3 oracle")),
        }
    }
}

impl SubjectFactory<ReferenceProtocol> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceProtocol, SubjectFailure>> {
        Box::pin(async { Ok(ReferenceProtocol) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceProtocol,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn protocol_catalog_runs_all_sixteen_cases() {
    let suite = protocol_suite::<ReferenceProtocol>();
    assert_eq!(suite.descriptor().id().as_str(), "peritus.protocol");
    assert_eq!(suite.cases().len(), 16);
    let ids = suite.cases().iter().map(|case| case.descriptor().id().as_str()).collect::<Vec<_>>();
    assert!(ids.windows(2).all(|pair| pair[0] < pair[1]));
    let report = block_on(ConformanceRunner::run(&suite, &Factory::new()));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 16);
}

#[test]
fn protocol_subject_type_is_independent_from_generic_runner_subject() {
    let _ = core::any::TypeId::of::<TestSubject>();
    let _ = core::any::TypeId::of::<ReferenceProtocol>();
}

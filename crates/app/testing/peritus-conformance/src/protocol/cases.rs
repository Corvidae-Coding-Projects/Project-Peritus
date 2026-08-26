//! Executable A3 application-protocol conformance cases.

use super::{ProtocolConformanceFixture, ProtocolConformanceSubject, ProtocolScenario};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct ProtocolCase {
    descriptor: CaseDescriptor,
    scenario: ProtocolScenario,
}

impl<S: ProtocolConformanceSubject> ConformanceCase<S> for ProtocolCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move { result(subject, self.scenario) })
    }
}

/// Returns the complete runtime-neutral A3 application-protocol suite.
#[must_use]
pub fn protocol_suite<S: ProtocolConformanceSubject + 'static>() -> StaticSuite<S> {
    use ProtocolScenario as P;
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.protocol"),
            ReportText::literal(
                "A3 negotiation, command, delivery, transfer, interaction, lifecycle, and bounds contract",
            ),
        ),
        vec![
            boxed(
                "ack-legality",
                "Acknowledgements remain within contiguous delivery",
                P::AckLegality,
            ),
            boxed(
                "artifact-transfer",
                "Artifact chunks conserve order, size, and digest",
                P::ArtifactTransfer,
            ),
            boxed(
                "backpressure",
                "In-flight limits pause without losing delivery",
                P::Backpressure,
            ),
            boxed("bounds", "Independent protocol limits fail closed", P::Bounds),
            boxed(
                "command-binding",
                "Commands retain exact actor, revision, and B3 bindings",
                P::CommandBinding,
            ),
            boxed(
                "daemon-lifecycle",
                "Readiness and shutdown states remain truthful",
                P::DaemonLifecycle,
            ),
            boxed(
                "gap-snapshot",
                "Retention gaps require explicit snapshot recovery",
                P::GapSnapshot,
            ),
            boxed(
                "idempotency",
                "Exact retries replay and changed key reuse conflicts",
                P::Idempotency,
            ),
            boxed("malformed", "Malformed and noncanonical frames remain inert", P::MalformedInput),
            boxed(
                "negotiation-downgraded",
                "Compatible downgrade is explicit",
                P::NegotiationDowngraded,
            ),
            boxed(
                "negotiation-exact",
                "Preferred common version negotiates exactly",
                P::NegotiationExact,
            ),
            boxed(
                "negotiation-incompatible",
                "Disjoint versions are incompatible",
                P::NegotiationIncompatible,
            ),
            boxed("prompt-freshness", "Prompt answers retain exact freshness", P::PromptFreshness),
            boxed(
                "required-feature",
                "Missing required features cannot create a session",
                P::RequiredFeature,
            ),
            boxed(
                "subscription-resume",
                "Resume and redelivery preserve event identity",
                P::SubscriptionResume,
            ),
            boxed(
                "terminal-ordering",
                "Terminal output precedes one final exit",
                P::TerminalOrdering,
            ),
        ],
    )
}

fn boxed<S: ProtocolConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    scenario: ProtocolScenario,
) -> Box<dyn ConformanceCase<S>> {
    Box::new(ProtocolCase {
        descriptor: CaseDescriptor::new(
            CaseId::new(format!("peritus.protocol.{suffix}"))
                .expect("static protocol case identifier"),
            ReportText::literal(summary),
        ),
        scenario,
    })
}

fn result<S: ProtocolConformanceSubject>(
    subject: &mut S,
    scenario: ProtocolScenario,
) -> CaseResult {
    let fixture = ProtocolConformanceFixture::new(scenario);
    let Ok(observed) = subject.exercise(&fixture) else {
        return failed(scenario, false, false);
    };
    let common =
        observed.expected_terminal && observed.stable_error_exact && observed.non_authoritative;
    let scenario_exact = match scenario {
        ProtocolScenario::NegotiationExact
        | ProtocolScenario::NegotiationDowngraded
        | ProtocolScenario::NegotiationIncompatible
        | ProtocolScenario::RequiredFeature => observed.negotiation_exact,
        ProtocolScenario::CommandBinding => observed.command_binding_exact,
        ProtocolScenario::Idempotency => observed.idempotency_exact,
        ProtocolScenario::SubscriptionResume | ProtocolScenario::AckLegality => {
            observed.delivery_exact
        }
        ProtocolScenario::GapSnapshot | ProtocolScenario::Backpressure => {
            observed.flow_control_exact
        }
        ProtocolScenario::ArtifactTransfer => observed.artifact_exact,
        ProtocolScenario::PromptFreshness => observed.prompt_exact,
        ProtocolScenario::TerminalOrdering => observed.terminal_exact,
        ProtocolScenario::DaemonLifecycle => observed.daemon_control_exact,
        ProtocolScenario::MalformedInput => observed.malformed_rejected,
        ProtocolScenario::Bounds => observed.bounds_enforced,
    };
    if common && scenario_exact {
        CaseResult::passed(observations(true, true))
    } else {
        failed(scenario, common, scenario_exact)
    }
}

fn failed(scenario: ProtocolScenario, common: bool, scenario_exact: bool) -> CaseResult {
    CaseResult::failed(observations(common, scenario_exact), assertion(scenario))
}

fn observations(common: bool, scenario_exact: bool) -> Vec<Observation> {
    vec![
        Observation::new(ObservationId::catalog("common"), ObservationValue::Boolean(common)),
        Observation::new(
            ObservationId::catalog("scenario-exact"),
            ObservationValue::Boolean(scenario_exact),
        ),
    ]
}

fn assertion(scenario: ProtocolScenario) -> AssertionFailure {
    let number = match scenario {
        ProtocolScenario::NegotiationExact => "001",
        ProtocolScenario::NegotiationDowngraded => "002",
        ProtocolScenario::NegotiationIncompatible => "003",
        ProtocolScenario::RequiredFeature => "004",
        ProtocolScenario::CommandBinding => "005",
        ProtocolScenario::Idempotency => "006",
        ProtocolScenario::SubscriptionResume => "007",
        ProtocolScenario::AckLegality => "008",
        ProtocolScenario::GapSnapshot => "009",
        ProtocolScenario::Backpressure => "010",
        ProtocolScenario::ArtifactTransfer => "011",
        ProtocolScenario::PromptFreshness => "012",
        ProtocolScenario::TerminalOrdering => "013",
        ProtocolScenario::DaemonLifecycle => "014",
        ProtocolScenario::MalformedInput => "015",
        ProtocolScenario::Bounds => "016",
    };
    AssertionFailure::new(
        FailureCode::new(format!("PERITUS-PROTOCOL-CONFORMANCE-{number}"))
            .expect("static protocol failure code"),
        ReportText::literal("A3 direct observations violated the selected protocol contract"),
        None,
        None,
    )
}

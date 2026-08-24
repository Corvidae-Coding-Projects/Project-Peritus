//! Executable C2/C3 sandbox conformance cases.

use super::fixtures::{
    ALL_DOMAINS, BACKEND_FEATURES, CANONICAL_FEATURES, ORDER_A, ORDER_B, SECRET_CANARY, fixture,
};
use super::{
    SandboxConformanceSubject, SandboxDecision, SandboxDomain, SandboxLifecyclePhase,
    SandboxPreparationFixture, SandboxScenario,
};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct SandboxCase {
    descriptor: CaseDescriptor,
    kind: SandboxCaseKind,
}

#[derive(Clone, Copy)]
enum SandboxCaseKind {
    DefaultDeny,
    Filesystem,
    EnvironmentSecret,
    Network,
    ProcessTerminal,
    ResourceBoundary,
    UnsupportedNoEffect,
    CancellationTeardown,
    ObservationBinding,
    CanonicalPreparation,
}

impl<S: SandboxConformanceSubject> ConformanceCase<S> for SandboxCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move {
            let exact = match self.kind {
                SandboxCaseKind::DefaultDeny => default_deny(subject),
                SandboxCaseKind::Filesystem => filesystem(subject),
                SandboxCaseKind::EnvironmentSecret => environment_secret(subject),
                SandboxCaseKind::Network => network(subject),
                SandboxCaseKind::ProcessTerminal => process_terminal(subject),
                SandboxCaseKind::ResourceBoundary => resource_boundary(subject),
                SandboxCaseKind::UnsupportedNoEffect => unsupported_no_effect(subject),
                SandboxCaseKind::CancellationTeardown => cancellation_teardown(subject),
                SandboxCaseKind::ObservationBinding => observation_binding(subject),
                SandboxCaseKind::CanonicalPreparation => canonical_preparation(subject),
            };
            result(exact, self.kind)
        })
    }
}

/// Returns the complete runtime-neutral C2/C3 sandbox conformance suite.
#[must_use]
pub fn sandbox_suite<S: SandboxConformanceSubject + 'static>() -> StaticSuite<S> {
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.sandbox"),
            ReportText::literal("C2/C3 complete sandbox policy, admission, and lifecycle contract"),
        ),
        vec![
            boxed(
                "cancellation-teardown",
                "Cancellation releases every backend resource",
                SandboxCaseKind::CancellationTeardown,
            ),
            boxed(
                "canonical-preparation",
                "Equivalent policy prepares canonically without effect",
                SandboxCaseKind::CanonicalPreparation,
            ),
            boxed(
                "default-deny",
                "An empty contract denies every capability domain",
                SandboxCaseKind::DefaultDeny,
            ),
            boxed(
                "environment-secret",
                "Explicit environment and secret reference stay confidential",
                SandboxCaseKind::EnvironmentSecret,
            ),
            boxed(
                "filesystem",
                "Filesystem deny rules dominate overlapping grants",
                SandboxCaseKind::Filesystem,
            ),
            boxed(
                "network",
                "Only the exact outbound destination is admitted",
                SandboxCaseKind::Network,
            ),
            boxed(
                "observation-binding",
                "Observations bind the exact plan in monotonic order",
                SandboxCaseKind::ObservationBinding,
            ),
            boxed(
                "process-terminal",
                "Descendant and terminal controls enforce exact bounds",
                SandboxCaseKind::ProcessTerminal,
            ),
            boxed(
                "resource-boundary",
                "Exact resource ceilings distinguish at-limit from over-limit",
                SandboxCaseKind::ResourceBoundary,
            ),
            boxed(
                "unsupported-no-effect",
                "Missing backend support fails closed before activation",
                SandboxCaseKind::UnsupportedNoEffect,
            ),
        ],
    )
}

fn boxed<S: SandboxConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    kind: SandboxCaseKind,
) -> Box<dyn ConformanceCase<S>> {
    Box::new(SandboxCase {
        descriptor: CaseDescriptor::new(
            CaseId::catalog(match suffix {
                "cancellation-teardown" => "peritus.sandbox.cancellation-teardown",
                "canonical-preparation" => "peritus.sandbox.canonical-preparation",
                "default-deny" => "peritus.sandbox.default-deny",
                "environment-secret" => "peritus.sandbox.environment-secret",
                "filesystem" => "peritus.sandbox.filesystem",
                "network" => "peritus.sandbox.network",
                "observation-binding" => "peritus.sandbox.observation-binding",
                "process-terminal" => "peritus.sandbox.process-terminal",
                "resource-boundary" => "peritus.sandbox.resource-boundary",
                _ => "peritus.sandbox.unsupported-no-effect",
            }),
            ReportText::literal(summary),
        ),
        kind,
    })
}

fn default_deny<S: SandboxConformanceSubject>(subject: &mut S) -> bool {
    subject.exercise(&fixture(SandboxScenario::DefaultDeny)).is_ok_and(|value| {
        value.decision() == SandboxDecision::Denied
            && value.denied_domains() == ALL_DOMAINS
            && value.live_effect_count() == 0
            && value.teardown_complete()
    })
}

fn filesystem<S: SandboxConformanceSubject>(subject: &mut S) -> bool {
    subject.exercise(&fixture(SandboxScenario::FilesystemDenyDominance)).is_ok_and(|value| {
        value.decision() == SandboxDecision::Denied
            && value.denied_domains() == [SandboxDomain::Filesystem]
            && value.live_effect_count() == 0
            && value.teardown_complete()
    })
}

fn environment_secret<S: SandboxConformanceSubject>(subject: &mut S) -> bool {
    let fixture = fixture(SandboxScenario::EnvironmentSecret);
    subject.exercise(&fixture).is_ok_and(|value| {
        value.decision() == SandboxDecision::Allowed
            && value.denied_domains().is_empty()
            && !contains_bytes(value.ordinary_observation_bytes(), fixture.secret_canary())
            && value.lifecycle() == SandboxLifecyclePhase::Released
            && value.teardown_complete()
    })
}

fn network<S: SandboxConformanceSubject>(subject: &mut S) -> bool {
    let allowed = subject.exercise(&fixture(SandboxScenario::NetworkAllowed));
    let denied = subject.exercise(&fixture(SandboxScenario::NetworkDenied));
    allowed.is_ok_and(|value| value.decision() == SandboxDecision::Allowed)
        && denied.is_ok_and(|value| {
            value.decision() == SandboxDecision::Denied
                && value.denied_domains() == [SandboxDomain::Network]
                && value.live_effect_count() == 0
        })
}

fn process_terminal<S: SandboxConformanceSubject>(subject: &mut S) -> bool {
    let within = subject.exercise(&fixture(SandboxScenario::ProcessTerminalWithin));
    let exceeded = subject.exercise(&fixture(SandboxScenario::ProcessTerminalExceeded));
    within.is_ok_and(|value| {
        value.decision() == SandboxDecision::Allowed
            && value.process_tree_contained()
            && value.terminal_controlled()
    }) && exceeded.is_ok_and(|value| {
        value.decision() == SandboxDecision::Violation
            && value.live_effect_count() == 0
            && value.teardown_complete()
    })
}

fn resource_boundary<S: SandboxConformanceSubject>(subject: &mut S) -> bool {
    let exact_fixture = fixture(SandboxScenario::ResourceAtLimit);
    let over_fixture = fixture(SandboxScenario::ResourceOverLimit);
    let exact = subject.exercise(&exact_fixture);
    let over = subject.exercise(&over_fixture);
    exact.is_ok_and(|value| {
        value.decision() == SandboxDecision::Allowed
            && value.resource_limit() == exact_fixture.resource_limit()
            && value.resource_observed() == exact_fixture.resource_requested()
    }) && over.is_ok_and(|value| {
        value.decision() == SandboxDecision::Violation
            && value.resource_limit() == over_fixture.resource_limit()
            && value.resource_observed() == over_fixture.resource_requested()
            && value.live_effect_count() == 0
    })
}

fn unsupported_no_effect<S: SandboxConformanceSubject>(subject: &mut S) -> bool {
    subject.exercise(&fixture(SandboxScenario::Unsupported)).is_ok_and(|value| {
        value.decision() == SandboxDecision::Unsupported
            && value.activation_count() == 0
            && value.live_effect_count() == 0
            && matches!(
                value.lifecycle(),
                SandboxLifecyclePhase::Planned | SandboxLifecyclePhase::Prepared
            )
    })
}

fn cancellation_teardown<S: SandboxConformanceSubject>(subject: &mut S) -> bool {
    subject.exercise(&fixture(SandboxScenario::Cancellation)).is_ok_and(|value| {
        value.decision() == SandboxDecision::Cancelled
            && value.cancellation_accepted()
            && value.lifecycle() == SandboxLifecyclePhase::Released
            && value.teardown_complete()
            && value.live_effect_count() == 0
    })
}

fn observation_binding<S: SandboxConformanceSubject>(subject: &mut S) -> bool {
    let fixture = fixture(SandboxScenario::ObservationBinding);
    subject.exercise(&fixture).is_ok_and(|value| {
        value.decision() == SandboxDecision::Allowed
            && value.plan_digest() == value.observation_plan_digest()
            && monotonic_nonzero(value.event_sequences())
            && !contains_bytes(value.ordinary_observation_bytes(), fixture.secret_canary())
    })
}

fn canonical_preparation<S: SandboxConformanceSubject>(subject: &mut S) -> bool {
    let first = subject.prepare(&SandboxPreparationFixture::new(
        ORDER_A,
        BACKEND_FEATURES,
        SECRET_CANARY,
        7,
    ));
    let second = subject.prepare(&SandboxPreparationFixture::new(
        ORDER_B,
        BACKEND_FEATURES,
        SECRET_CANARY,
        7,
    ));
    let drift = subject.prepare(&SandboxPreparationFixture::new(
        ORDER_A,
        BACKEND_FEATURES,
        SECRET_CANARY,
        8,
    ));
    let (Ok(first), Ok(second), Ok(drift)) = (first, second, drift) else { return false };
    first.admitted()
        && second.admitted()
        && drift.admitted()
        && first.missing_features().is_empty()
        && first.canonical_features() == CANONICAL_FEATURES
        && first.canonical_features() == second.canonical_features()
        && first.canonical_bytes() == second.canonical_bytes()
        && first.plan_digest() == second.plan_digest()
        && first.preparation_digest() == second.preparation_digest()
        && first.plan_digest() != drift.plan_digest()
        && first.preparation_digest() != drift.preparation_digest()
        && !contains_bytes(first.canonical_bytes(), SECRET_CANARY)
        && !contains_bytes(second.canonical_bytes(), SECRET_CANARY)
        && !contains_bytes(drift.canonical_bytes(), SECRET_CANARY)
        && first.native_effect_count() == 0
        && second.native_effect_count() == 0
        && drift.native_effect_count() == 0
}

fn result(exact: bool, kind: SandboxCaseKind) -> CaseResult {
    let observations =
        vec![Observation::new(ObservationId::catalog("exact"), ObservationValue::Boolean(exact))];
    if exact {
        CaseResult::passed(observations)
    } else {
        CaseResult::failed(observations, assertion(kind))
    }
}

fn assertion(kind: SandboxCaseKind) -> AssertionFailure {
    let (code, summary) = match kind {
        SandboxCaseKind::DefaultDeny => {
            ("001", "default denial did not cover every sandbox domain")
        }
        SandboxCaseKind::Filesystem => {
            ("002", "filesystem denial did not dominate the overlapping grant")
        }
        SandboxCaseKind::EnvironmentSecret => {
            ("003", "environment or secret-reference enforcement was incomplete")
        }
        SandboxCaseKind::Network => {
            ("004", "network policy did not distinguish the exact destination")
        }
        SandboxCaseKind::ProcessTerminal => {
            ("005", "process-tree or terminal bounds were not enforced")
        }
        SandboxCaseKind::ResourceBoundary => {
            ("006", "resource accounting did not enforce the exact boundary")
        }
        SandboxCaseKind::UnsupportedNoEffect => {
            ("007", "unsupported enforcement activated or produced an effect")
        }
        SandboxCaseKind::CancellationTeardown => {
            ("008", "cancellation did not complete backend teardown")
        }
        SandboxCaseKind::ObservationBinding => {
            ("009", "sandbox observations were not ordered and plan-bound")
        }
        SandboxCaseKind::CanonicalPreparation => {
            ("010", "canonical preparation changed with ordering or caused an effect")
        }
    };
    AssertionFailure::new(
        FailureCode::catalog(match code {
            "001" => "PERITUS-SANDBOX-CONFORMANCE-001",
            "002" => "PERITUS-SANDBOX-CONFORMANCE-002",
            "003" => "PERITUS-SANDBOX-CONFORMANCE-003",
            "004" => "PERITUS-SANDBOX-CONFORMANCE-004",
            "005" => "PERITUS-SANDBOX-CONFORMANCE-005",
            "006" => "PERITUS-SANDBOX-CONFORMANCE-006",
            "007" => "PERITUS-SANDBOX-CONFORMANCE-007",
            "008" => "PERITUS-SANDBOX-CONFORMANCE-008",
            "009" => "PERITUS-SANDBOX-CONFORMANCE-009",
            _ => "PERITUS-SANDBOX-CONFORMANCE-010",
        }),
        ReportText::literal(summary),
        None,
        None,
    )
}

fn monotonic_nonzero(values: &[u64]) -> bool {
    values.first().is_some_and(|value| *value > 0)
        && values.windows(2).all(|pair| pair[0] < pair[1])
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && haystack.windows(needle.len()).any(|window| window == needle)
}

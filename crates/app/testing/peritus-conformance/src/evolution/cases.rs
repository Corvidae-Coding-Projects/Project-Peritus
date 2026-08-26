//! Executable F0 evolution conformance cases.

use super::{
    EvolutionConformanceFixture, EvolutionConformanceSubject, EvolutionScenario, EvolutionTerminal,
};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct EvolutionCase {
    descriptor: CaseDescriptor,
    scenario: EvolutionScenario,
}

impl<S: EvolutionConformanceSubject> ConformanceCase<S> for EvolutionCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move { result(subject, self.scenario) })
    }
}

/// Returns the complete runtime-neutral F0 conformance suite.
#[must_use]
pub fn evolution_suite<S: EvolutionConformanceSubject + 'static>() -> StaticSuite<S> {
    use EvolutionScenario as E;
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.evolution"),
            ReportText::literal(
                "F0 immutable evidence, deterministic selection, authority, atomic activation, and rollback contract",
            ),
        ),
        vec![
            boxed("activation", "Production activation is atomic", E::AtomicActivation),
            boxed(
                "authority",
                "Human promotion authority is exact and single-use",
                E::HumanAuthority,
            ),
            boxed("bounds", "Independent evolution limits fail closed", E::Bounds),
            boxed("changes", "Component changes preserve protected assets", E::ChangeIsolation),
            boxed("contamination", "Contaminated evidence cannot promote", E::Contamination),
            boxed("evidence", "Evolution evidence remains frozen and exact", E::FrozenEvidence),
            boxed(
                "interaction",
                "Interacting changes retain group attribution",
                E::InteractionAttribution,
            ),
            boxed("malformed", "Malformed evolution frames stay inert", E::MalformedInput),
            boxed("metric-gaming", "Mandatory failures cannot be offset", E::MetricGaming),
            boxed("replay", "Replay avoids duplicate promotion authority", E::DurableReplay),
            boxed("review", "Executable changes require independent review", E::IndependentReview),
            boxed("rollback", "Rollback appends auditable history", E::RollbackHistory),
            boxed("selection", "Eligible selection is deterministic", E::DeterministicSelection),
            boxed("stale", "Stale evidence and baselines reject", E::StaleEvidence),
        ],
    )
}

fn boxed<S: EvolutionConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    scenario: EvolutionScenario,
) -> Box<dyn ConformanceCase<S>> {
    Box::new(EvolutionCase {
        descriptor: CaseDescriptor::new(
            CaseId::new(format!("peritus.evolution.{suffix}"))
                .expect("static evolution case identifier"),
            ReportText::literal(summary),
        ),
        scenario,
    })
}

fn result<S: EvolutionConformanceSubject>(
    subject: &mut S,
    scenario: EvolutionScenario,
) -> CaseResult {
    let fixture = EvolutionConformanceFixture::new(scenario);
    let Ok(observed) = subject.exercise(&fixture) else {
        return failed(scenario, [0; 4], false);
    };
    let counts = [
        u64::from(observed.manifests),
        u64::from(observed.variants),
        u64::from(observed.criteria),
        u64::from(observed.activation_history),
    ];
    let bounded = observed.manifests <= fixture.maximum_manifests()
        && observed.variants <= fixture.maximum_variants()
        && observed.criteria <= fixture.maximum_criteria()
        && observed.activation_history <= fixture.maximum_activation_history()
        && observed.bounds_enforced;
    let promoted = observed.terminal == EvolutionTerminal::Promoted;
    let rejected = observed.terminal == EvolutionTerminal::Rejected;
    let rolled_back = observed.terminal == EvolutionTerminal::RolledBack;
    let common = bounded && observed.evidence_exact && observed.non_self_promoting;
    let exact = common
        && match scenario {
            EvolutionScenario::FrozenEvidence => promoted && observed.frozen_evidence_exact,
            EvolutionScenario::ChangeIsolation => promoted && observed.change_isolation_exact,
            EvolutionScenario::InteractionAttribution => {
                promoted && observed.interaction_attribution_exact
            }
            EvolutionScenario::Contamination => rejected && observed.contamination_rejected,
            EvolutionScenario::MetricGaming => rejected && observed.metric_gaming_rejected,
            EvolutionScenario::DeterministicSelection => {
                promoted && observed.selection_deterministic
            }
            EvolutionScenario::StaleEvidence => rejected && observed.stale_evidence_rejected,
            EvolutionScenario::IndependentReview => promoted && observed.review_exact,
            EvolutionScenario::HumanAuthority => promoted && observed.authority_exact,
            EvolutionScenario::AtomicActivation => promoted && observed.activation_atomic,
            EvolutionScenario::RollbackHistory => rolled_back && observed.rollback_auditable,
            EvolutionScenario::DurableReplay => promoted && observed.replay_equivalent,
            EvolutionScenario::MalformedInput => rejected && observed.malformed_rejected,
            EvolutionScenario::Bounds => rejected && observed.bounds_enforced,
        };
    if exact {
        CaseResult::passed(observations(counts, true))
    } else {
        failed(scenario, counts, false)
    }
}

fn failed(scenario: EvolutionScenario, counts: [u64; 4], exact: bool) -> CaseResult {
    CaseResult::failed(observations(counts, exact), assertion(scenario))
}

fn observations(counts: [u64; 4], exact: bool) -> Vec<Observation> {
    ["manifests", "variants", "criteria", "activation-history"]
        .into_iter()
        .zip(counts)
        .map(|(name, value)| {
            Observation::new(ObservationId::catalog(name), ObservationValue::Unsigned(value))
        })
        .chain([Observation::new(
            ObservationId::catalog("exact"),
            ObservationValue::Boolean(exact),
        )])
        .collect()
}

fn assertion(scenario: EvolutionScenario) -> AssertionFailure {
    let number = match scenario {
        EvolutionScenario::FrozenEvidence => "001",
        EvolutionScenario::ChangeIsolation => "002",
        EvolutionScenario::InteractionAttribution => "003",
        EvolutionScenario::Contamination => "004",
        EvolutionScenario::MetricGaming => "005",
        EvolutionScenario::DeterministicSelection => "006",
        EvolutionScenario::StaleEvidence => "007",
        EvolutionScenario::IndependentReview => "008",
        EvolutionScenario::HumanAuthority => "009",
        EvolutionScenario::AtomicActivation => "010",
        EvolutionScenario::RollbackHistory => "011",
        EvolutionScenario::DurableReplay => "012",
        EvolutionScenario::MalformedInput => "013",
        EvolutionScenario::Bounds => "014",
    };
    AssertionFailure::new(
        FailureCode::new(format!("PERITUS-EVOLUTION-CONFORMANCE-{number}"))
            .expect("static evolution failure code"),
        ReportText::literal("F0 direct observations violated the selected evolution contract"),
        None,
        None,
    )
}

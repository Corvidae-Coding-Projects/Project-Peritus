//! Executable E1 harness materialization conformance cases.

use super::{
    HarnessConformanceFixture, HarnessConformanceSubject, HarnessScenario, HarnessTerminal,
};
use crate::{
    AssertionFailure, CaseDescriptor, CaseId, CaseResult, ConformanceCase, ConformanceFuture,
    FailureCode, Observation, ObservationId, ObservationValue, ReportText, StaticSuite,
    SuiteDescriptor, SuiteId,
};

struct HarnessCase {
    descriptor: CaseDescriptor,
    scenario: HarnessScenario,
}

impl<S: HarnessConformanceSubject> ConformanceCase<S> for HarnessCase {
    fn descriptor(&self) -> &CaseDescriptor {
        &self.descriptor
    }

    fn run<'a>(&'a self, subject: &'a mut S) -> ConformanceFuture<'a, CaseResult> {
        Box::pin(async move { result(subject, self.scenario) })
    }
}

/// Returns the complete runtime-neutral E1 conformance suite.
#[must_use]
pub fn harness_suite<S: HarnessConformanceSubject + 'static>() -> StaticSuite<S> {
    use HarnessScenario as H;
    StaticSuite::new(
        SuiteDescriptor::new(
            SuiteId::catalog("peritus.harness"),
            ReportText::literal(
                "E1 manifest, graph, revision, materialization, rollback, and replay contract",
            ),
        ),
        vec![
            boxed(
                "artifact-integrity",
                "Materialization reverifies finalized artifacts",
                H::ArtifactIntegrity,
            ),
            boxed(
                "authority",
                "Component authority cannot widen through dependencies",
                H::AuthorityConfinement,
            ),
            boxed(
                "bounds",
                "All independent harness state bounds remain explicit",
                H::BoundedState,
            ),
            boxed("catalog", "Every component and protected class is modeled", H::CompleteCatalog),
            boxed(
                "forward",
                "Forward materialization changes only exact owned paths",
                H::ForwardMaterialization,
            ),
            boxed(
                "graph",
                "Invalid graph and compatibility forms are rejected",
                H::GraphCompatibility,
            ),
            boxed(
                "malformed-protocol",
                "Malformed harness frames stay inert",
                H::MalformedProtocol,
            ),
            boxed(
                "manifest",
                "Manifest declarations equal the C1 source inventory",
                H::ManifestInventory,
            ),
            boxed("panic", "Subject panic is contained as failure", H::PanicContainment),
            boxed(
                "protected",
                "Protected controlled assets remain immutable",
                H::ProtectedImmutability,
            ),
            boxed("restart", "Replay and exact retry avoid duplicate effects", H::Restart),
            boxed("revision", "Content-addressed history remains append-only", H::RevisionHistory),
            boxed(
                "rollback",
                "Rollback accepts only an immutable ancestor",
                H::RollbackMaterialization,
            ),
            boxed("teardown", "Teardown failure remains explicit", H::TeardownIsolation),
        ],
    )
}

fn boxed<S: HarnessConformanceSubject + 'static>(
    suffix: &'static str,
    summary: &'static str,
    scenario: HarnessScenario,
) -> Box<dyn ConformanceCase<S>> {
    Box::new(HarnessCase {
        descriptor: CaseDescriptor::new(
            CaseId::new(format!("peritus.harness.{suffix}"))
                .expect("static harness case identifier"),
            ReportText::literal(summary),
        ),
        scenario,
    })
}

fn result<S: HarnessConformanceSubject>(subject: &mut S, scenario: HarnessScenario) -> CaseResult {
    let fixture = HarnessConformanceFixture::new(scenario);
    let Ok(observed) = subject.exercise(&fixture) else {
        return failed(scenario, [0; 4], false);
    };
    let counts = [observed.components, observed.edges, observed.revisions, observed.receipts];
    let bounded = observed.components <= fixture.maximum_components()
        && observed.edges <= fixture.maximum_edges()
        && observed.revisions <= fixture.maximum_revisions()
        && observed.receipts <= fixture.maximum_receipts();
    let common = bounded && observed.bounds_enforced && observed.no_implicit_promotion;
    let completed = observed.terminal == HarnessTerminal::Completed;
    let exact = common
        && match scenario {
            HarnessScenario::ManifestInventory => completed && observed.manifest_inventory_exact,
            HarnessScenario::CompleteCatalog => completed && observed.catalog_complete,
            HarnessScenario::GraphCompatibility => !completed && observed.graph_rejections_exact,
            HarnessScenario::AuthorityConfinement => !completed && observed.authority_confined,
            HarnessScenario::ProtectedImmutability => !completed && observed.protected_immutable,
            HarnessScenario::RevisionHistory => completed && observed.revision_history_exact,
            HarnessScenario::ForwardMaterialization => {
                completed
                    && observed.workspace_materialization_exact
                    && observed.unrelated_paths_preserved
            }
            HarnessScenario::RollbackMaterialization => completed && observed.rollback_exact,
            HarnessScenario::ArtifactIntegrity => completed && observed.artifacts_verified,
            HarnessScenario::BoundedState => !completed && observed.bounds_enforced,
            HarnessScenario::Restart => observed.replay_equivalent && observed.idempotent_recovery,
            HarnessScenario::MalformedProtocol => !completed && observed.malformed_rejected,
            HarnessScenario::PanicContainment => observed.panic_contained,
            HarnessScenario::TeardownIsolation => observed.teardown_explicit,
        };
    if exact {
        CaseResult::passed(observations(counts, true))
    } else {
        failed(scenario, counts, bounded)
    }
}

fn failed(scenario: HarnessScenario, counts: [u16; 4], exact: bool) -> CaseResult {
    CaseResult::failed(observations(counts, exact), assertion(scenario))
}

fn observations(counts: [u16; 4], exact: bool) -> Vec<Observation> {
    let names = ["components", "edges", "revisions", "receipts"];
    let mut observations = names
        .into_iter()
        .zip(counts)
        .map(|(name, value)| {
            Observation::new(
                ObservationId::catalog(name),
                ObservationValue::Unsigned(u64::from(value)),
            )
        })
        .collect::<Vec<_>>();
    observations
        .push(Observation::new(ObservationId::catalog("exact"), ObservationValue::Boolean(exact)));
    observations
}

fn assertion(scenario: HarnessScenario) -> AssertionFailure {
    let number = match scenario {
        HarnessScenario::ManifestInventory => "001",
        HarnessScenario::CompleteCatalog => "002",
        HarnessScenario::GraphCompatibility => "003",
        HarnessScenario::AuthorityConfinement => "004",
        HarnessScenario::ProtectedImmutability => "005",
        HarnessScenario::RevisionHistory => "006",
        HarnessScenario::ForwardMaterialization => "007",
        HarnessScenario::RollbackMaterialization => "008",
        HarnessScenario::ArtifactIntegrity => "009",
        HarnessScenario::BoundedState => "010",
        HarnessScenario::Restart => "011",
        HarnessScenario::MalformedProtocol => "012",
        HarnessScenario::PanicContainment => "013",
        HarnessScenario::TeardownIsolation => "014",
    };
    AssertionFailure::new(
        FailureCode::new(format!("PERITUS-HARNESS-CONFORMANCE-{number}"))
            .expect("static harness failure code"),
        ReportText::literal("E1 direct observations violated the selected harness contract"),
        None,
        None,
    )
}

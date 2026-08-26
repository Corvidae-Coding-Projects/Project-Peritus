use peritus_conformance::{
    CaseDescriptor, CaseStatus, ConformanceFuture, ConformanceRunner, HarnessConformanceError,
    HarnessConformanceFixture, HarnessConformanceObservation, HarnessConformanceSubject,
    HarnessScenario, HarnessTerminal, SubjectDescriptor, SubjectFactory, SubjectFailure,
    SuiteStatus, harness_suite,
};

use super::harness::{block_on, text};

struct ReferenceHarness {
    violate_protection: bool,
}

impl HarnessConformanceSubject for ReferenceHarness {
    fn exercise(
        &mut self,
        fixture: &HarnessConformanceFixture,
    ) -> Result<HarnessConformanceObservation, HarnessConformanceError> {
        let rejected = matches!(
            fixture.scenario(),
            HarnessScenario::GraphCompatibility
                | HarnessScenario::AuthorityConfinement
                | HarnessScenario::ProtectedImmutability
                | HarnessScenario::BoundedState
                | HarnessScenario::MalformedProtocol
        );
        Ok(HarnessConformanceObservation {
            terminal: if rejected { HarnessTerminal::Rejected } else { HarnessTerminal::Completed },
            components: 12,
            edges: 18,
            revisions: 3,
            receipts: 2,
            manifest_inventory_exact: true,
            catalog_complete: true,
            graph_rejections_exact: true,
            authority_confined: true,
            protected_immutable: !self.violate_protection,
            revision_history_exact: true,
            workspace_materialization_exact: true,
            unrelated_paths_preserved: true,
            rollback_exact: true,
            artifacts_verified: true,
            bounds_enforced: true,
            replay_equivalent: true,
            idempotent_recovery: true,
            malformed_rejected: true,
            panic_contained: true,
            teardown_explicit: true,
            no_implicit_promotion: true,
        })
    }
}

struct Factory {
    descriptor: SubjectDescriptor,
    violate_protection: bool,
}

impl Factory {
    fn new(violate_protection: bool) -> Self {
        Self {
            descriptor: SubjectDescriptor::new(text("harness-reference"), text("A2 E1 oracle")),
            violate_protection,
        }
    }
}

impl SubjectFactory<ReferenceHarness> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceHarness, SubjectFailure>> {
        let violate_protection = self.violate_protection;
        Box::pin(async move { Ok(ReferenceHarness { violate_protection }) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceHarness,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn harness_catalog_runs_all_fourteen_cases() {
    let report = block_on(ConformanceRunner::run(
        &harness_suite::<ReferenceHarness>(),
        &Factory::new(false),
    ));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 14);
}

#[test]
fn harness_catalog_rejects_protected_asset_drift() {
    let report =
        block_on(ConformanceRunner::run(&harness_suite::<ReferenceHarness>(), &Factory::new(true)));
    assert!(report.cases().iter().any(|case| {
        case.status() == CaseStatus::Failed
            && case.descriptor().id().as_str() == "peritus.harness.protected"
    }));
}

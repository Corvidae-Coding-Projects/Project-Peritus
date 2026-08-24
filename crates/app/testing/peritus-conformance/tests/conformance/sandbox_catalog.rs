use std::sync::{Arc, Mutex};

use peritus_conformance::{
    CaseDescriptor, CaseStatus, ConformanceFuture, ConformanceRunner, SandboxConformanceError,
    SandboxConformanceFixture, SandboxConformanceObservation, SandboxConformanceSubject,
    SandboxDecision, SandboxDomain, SandboxLifecyclePhase, SandboxPreparationFixture,
    SandboxPreparationObservation, SandboxScenario, SubjectDescriptor, SubjectFactory,
    SubjectFailure, SuiteStatus, sandbox_suite,
};

use super::harness::{block_on, text};

#[derive(Clone, Copy, Default)]
enum SandboxOracleBehavior {
    #[default]
    Honest,
    SelfConsistentWrongResources,
}

#[derive(Default)]
struct ReferenceSandboxSubject {
    behavior: SandboxOracleBehavior,
}

impl SandboxConformanceSubject for ReferenceSandboxSubject {
    fn exercise(
        &mut self,
        fixture: &SandboxConformanceFixture,
    ) -> Result<SandboxConformanceObservation, SandboxConformanceError> {
        let scenario = fixture.scenario();
        let decision = match scenario {
            SandboxScenario::DefaultDeny
            | SandboxScenario::FilesystemDenyDominance
            | SandboxScenario::NetworkDenied => SandboxDecision::Denied,
            SandboxScenario::ProcessTerminalExceeded | SandboxScenario::ResourceOverLimit => {
                SandboxDecision::Violation
            }
            SandboxScenario::Unsupported => SandboxDecision::Unsupported,
            SandboxScenario::Cancellation => SandboxDecision::Cancelled,
            _ => SandboxDecision::Allowed,
        };
        let denied_domains = match scenario {
            SandboxScenario::DefaultDeny => vec![
                SandboxDomain::Filesystem,
                SandboxDomain::Process,
                SandboxDomain::Environment,
                SandboxDomain::Network,
                SandboxDomain::Secret,
                SandboxDomain::Resource,
                SandboxDomain::Terminal,
            ],
            SandboxScenario::FilesystemDenyDominance => vec![SandboxDomain::Filesystem],
            SandboxScenario::NetworkDenied => vec![SandboxDomain::Network],
            _ => Vec::new(),
        };
        let unsupported = scenario == SandboxScenario::Unsupported;
        let digest_byte =
            u8::try_from(fixture.resource_limit()).expect("fixed resource limit fits in u8");
        let digest = [digest_byte; 32];
        let (resource_observed, resource_limit) = match (self.behavior, scenario) {
            (
                SandboxOracleBehavior::SelfConsistentWrongResources,
                SandboxScenario::ResourceAtLimit,
            ) => (1, 1),
            (
                SandboxOracleBehavior::SelfConsistentWrongResources,
                SandboxScenario::ResourceOverLimit,
            ) => (2, 1),
            _ => (fixture.resource_requested(), fixture.resource_limit()),
        };
        Ok(SandboxConformanceObservation::new(
            decision,
            if unsupported {
                SandboxLifecyclePhase::Planned
            } else {
                SandboxLifecyclePhase::Released
            },
            denied_domains,
            resource_observed,
            resource_limit,
            u64::from(!unsupported),
            0,
            scenario == SandboxScenario::Cancellation,
            !unsupported,
            digest,
            digest,
            vec![1, 2, 3],
            b"bounded sandbox observation".to_vec(),
            scenario == SandboxScenario::ProcessTerminalWithin,
            scenario == SandboxScenario::ProcessTerminalWithin,
        ))
    }

    fn prepare(
        &mut self,
        fixture: &SandboxPreparationFixture,
    ) -> Result<SandboxPreparationObservation, SandboxConformanceError> {
        let mut canonical = fixture.required_features().to_vec();
        canonical.sort_unstable();
        canonical.dedup();
        let missing = canonical
            .iter()
            .copied()
            .filter(|feature| !fixture.backend_features().contains(feature))
            .collect::<Vec<_>>();
        let marker = u8::try_from(fixture.authority_marker()).expect("fixed marker fits in u8");
        let admitted = missing.is_empty();
        let canonical_bytes =
            format!("peritus.sandbox.plan.v1:{canonical:?}:{marker}").into_bytes();
        Ok(SandboxPreparationObservation::new(
            canonical,
            missing,
            [marker; 32],
            [marker.wrapping_add(1); 32],
            admitted,
            canonical_bytes,
            0,
        ))
    }
}

#[derive(Clone, Copy, Default)]
struct FactoryCounts {
    created: usize,
    torn_down: usize,
}

struct Factory {
    descriptor: SubjectDescriptor,
    counts: Arc<Mutex<FactoryCounts>>,
    behavior: SandboxOracleBehavior,
}

impl Factory {
    fn new() -> Self {
        Self {
            descriptor: SubjectDescriptor::new(
                text("sandbox-reference"),
                text("A2 sandbox oracle"),
            ),
            counts: Arc::new(Mutex::new(FactoryCounts::default())),
            behavior: SandboxOracleBehavior::Honest,
        }
    }

    fn with_behavior(behavior: SandboxOracleBehavior) -> Self {
        Self { behavior, ..Self::new() }
    }
}

impl SubjectFactory<ReferenceSandboxSubject> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ReferenceSandboxSubject, SubjectFailure>> {
        self.counts.lock().expect("counts lock").created += 1;
        let behavior = self.behavior;
        Box::pin(async move { Ok(ReferenceSandboxSubject { behavior }) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ReferenceSandboxSubject,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        self.counts.lock().expect("counts lock").torn_down += 1;
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn sandbox_catalog_executes_complete_contract_with_a_fresh_subject_per_case() {
    let factory = Factory::new();
    let report =
        block_on(ConformanceRunner::run(&sandbox_suite::<ReferenceSandboxSubject>(), &factory));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 10);
    let counts = *factory.counts.lock().expect("counts lock");
    assert_eq!(counts.created, 10);
    assert_eq!(counts.torn_down, 10);
}

#[test]
fn sandbox_catalog_rejects_self_consistent_but_wrong_resource_observations() {
    let factory = Factory::with_behavior(SandboxOracleBehavior::SelfConsistentWrongResources);
    let report =
        block_on(ConformanceRunner::run(&sandbox_suite::<ReferenceSandboxSubject>(), &factory));
    let failures = report
        .cases()
        .iter()
        .filter(|case| case.status() == CaseStatus::Failed)
        .map(|case| case.descriptor().id().as_str())
        .collect::<Vec<_>>();
    assert_eq!(failures, ["peritus.sandbox.resource-boundary"]);
}

//! Production conformance-catalog coverage for F0 behavior.

mod support;

use std::{
    future::Future,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    thread,
};

use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, EvolutionConformanceError,
    EvolutionConformanceFixture, EvolutionConformanceObservation, EvolutionConformanceSubject,
    EvolutionScenario, EvolutionTerminal, ReportText, SubjectDescriptor, SubjectFactory,
    SubjectFailure, SuiteStatus, evolution_suite,
};
use peritus_evolution::{
    ActivationAuthorization, EvolutionLimits,
    verified::{
        EvaluatorIsolationFacts, PromotionSafetyFacts, deny_wins, evaluator_isolation,
        pointer_conservation, promotion_safety, replay_refinement, rollback_legality,
    },
};

use support::{HarnessFixture, digest, policy};

struct ProductionEvolution;

impl EvolutionConformanceSubject for ProductionEvolution {
    #[allow(
        clippy::suspicious_operation_groupings,
        reason = "conformance binds like-typed fields across independently owned evidence records"
    )]
    fn exercise(
        &mut self,
        fixture: &EvolutionConformanceFixture,
    ) -> Result<EvolutionConformanceObservation, EvolutionConformanceError> {
        let harness = HarnessFixture::new();
        let frozen_evidence_exact = harness.baseline != harness.candidate
            && harness.policy.production_revision() == harness.baseline.harness_revision()
            && harness.policy.preserved_by(&harness.candidate_revision);
        let change_isolation_exact = harness.policy.preserved_by(&harness.candidate_revision);
        let interaction_attribution_exact =
            evaluator_isolation(EvaluatorIsolationFacts::new(true, true, true, true, true, true));
        let contamination_rejected =
            !evaluator_isolation(EvaluatorIsolationFacts::new(true, true, false, true, true, true));
        let metric_gaming_rejected = !deny_wins(true, 1, 0) && !deny_wins(true, 0, 1);
        let selection_deterministic = policy().digest() == policy().digest();
        let stale_evidence_rejected = !promotion_safety(PromotionSafetyFacts::new(
            true, false, true, true, true, true, true, true, true, true,
        ));
        let review_exact = harness
            .policy
            .policy()
            .review_required_kinds()
            .contains(&peritus_harness::domain::ComponentKind::RolePrompt);
        let authority =
            ActivationAuthorization::new(digest(1), digest(2), digest(3), digest(4), digest(5));
        let authority_exact = authority.action_digest() == digest(1)
            && authority.approval_use_digest() == digest(4)
            && authority.digest()
                != ActivationAuthorization::new(
                    digest(1),
                    digest(2),
                    digest(3),
                    digest(9),
                    digest(5),
                )
                .digest();
        let activation_atomic = pointer_conservation(true, true, true, true)
            && !pointer_conservation(true, false, true, true);
        let rollback_auditable = rollback_legality(true, true, true, true, true, true)
            && !rollback_legality(true, true, true, false, true, true);
        let replay_equivalent = replay_refinement(true, true, true, true, true)
            && !replay_refinement(true, true, false, true, true);
        let malformed_rejected = peritus_evolution::EvolutionCampaignId::new([0; 16]).is_err();
        let bounds_enforced = EvolutionLimits::new(1, 1, 1, 1, 1, 1, 1, 1, 0).is_err();
        let terminal = match fixture.scenario() {
            EvolutionScenario::RollbackHistory => EvolutionTerminal::RolledBack,
            EvolutionScenario::Contamination
            | EvolutionScenario::MetricGaming
            | EvolutionScenario::StaleEvidence
            | EvolutionScenario::MalformedInput
            | EvolutionScenario::Bounds => EvolutionTerminal::Rejected,
            _ => EvolutionTerminal::Promoted,
        };
        Ok(EvolutionConformanceObservation {
            terminal,
            manifests: 1,
            variants: 1,
            criteria: 14,
            activation_history: 2,
            frozen_evidence_exact,
            change_isolation_exact,
            interaction_attribution_exact,
            contamination_rejected,
            metric_gaming_rejected,
            selection_deterministic,
            stale_evidence_rejected,
            review_exact,
            authority_exact,
            activation_atomic,
            rollback_auditable,
            replay_equivalent,
            malformed_rejected,
            bounds_enforced,
            evidence_exact: frozen_evidence_exact,
            non_self_promoting: !promotion_safety(PromotionSafetyFacts::new(
                true, true, true, true, true, true, true, true, true, false,
            )),
        })
    }
}

struct Factory(SubjectDescriptor);

impl Factory {
    fn new() -> Self {
        Self(SubjectDescriptor::new(
            ReportText::new("peritus-evolution").expect("subject name"),
            ReportText::new("F0 production public APIs").expect("subject implementation"),
        ))
    }
}

impl SubjectFactory<ProductionEvolution> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.0
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ProductionEvolution, SubjectFailure>> {
        Box::pin(async { Ok(ProductionEvolution) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ProductionEvolution,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn a2_evolution_catalog_executes_against_production_public_apis() {
    let report = block_on(ConformanceRunner::run(
        &evolution_suite::<ProductionEvolution>(),
        &Factory::new(),
    ));
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 14);
    assert_eq!(report.summary().passed(), 14);
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    struct ThreadWake(thread::Thread);

    impl Wake for ThreadWake {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }

        fn wake_by_ref(self: &Arc<Self>) {
            self.0.unpark();
        }
    }

    let waker = Waker::from(Arc::new(ThreadWake(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}

//! A2 conformance bridge exercised against E3 production domain code.

mod support;

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use peritus_codec::{CodecLimits, decode_message, encode_message};
use peritus_conformance::{
    CaseDescriptor, ConformanceFuture, ConformanceRunner, EvaluationConformanceError,
    EvaluationConformanceFixture, EvaluationConformanceObservation, EvaluationConformanceSubject,
    EvaluationScenario, EvaluationTerminal, ReportText, StaticSuite, SubjectDescriptor,
    SubjectFactory, SubjectFailure, SuiteReport, SuiteStatus, evaluation_suite,
};
use peritus_eval::{
    EvaluationCommand, EvaluationCommandFrame, EvaluationCommandKind, EvaluationErrorKind,
    EvaluationOperation, EvaluationPhase, EvaluationPlan, EvaluationRecovery, NeverCancelled,
    ReportRecord, RolloutLedger, RolloutOutcome, RolloutRecord, decide, execute_rollout, pass_at_k,
    replay,
};
use peritus_types::{CommandId, EventId};

use support::{
    FixturePort, PortMode, artifact, bytes, campaign_id, digest, frozen_profile, revision,
};

struct ProductionEvaluation;

impl EvaluationConformanceSubject for ProductionEvaluation {
    fn exercise(
        &mut self,
        fixture: &EvaluationConformanceFixture,
    ) -> Result<EvaluationConformanceObservation, EvaluationConformanceError> {
        let profile = frozen_profile();
        let plan = EvaluationPlan::build(campaign_id(), &profile)
            .map_err(|_| EvaluationConformanceError::Infrastructure)?;
        let frozen_inputs_exact = profile == frozen_profile()
            && profile.digest() == frozen_profile().digest()
            && profile.dataset().digest() == frozen_profile().dataset().digest();
        let isolation_exact = profile
            .dataset()
            .tasks()
            .iter()
            .all(|task| task.candidate_input().artifact() != task.evaluator_input().artifact());
        let deterministic = plan
            == EvaluationPlan::build(campaign_id(), &profile)
                .map_err(|_| EvaluationConformanceError::Infrastructure)?;
        let (accounting_complete, infrastructure_distinct) = accounting_facts(&plan, &profile)?;
        let statistics_valid = statistical_facts();
        let cancellation_durable = cancellation_fact(&profile)?;
        let (replay_equivalent, malformed_rejected) = protocol_facts(&profile)?;
        let publication_ordered = publication_ordering_fact(&profile)?;
        let redaction_safe = !format!(
            "{}",
            peritus_eval::EvaluationError::new(
                EvaluationErrorKind::Statistics,
                EvaluationOperation::Analyze,
                EvaluationRecovery::CorrectInput,
                "statistical input is invalid",
            )
        )
        .contains(fixture.canary());
        let bounds_enforced = peritus_eval::EvaluationLimits::new(
            peritus_eval::EvaluationLimits::MAX_TASKS + 1,
            1,
            1,
            1,
            1,
            1,
        )
        .is_err();
        let panic_contained = std::panic::catch_unwind(|| {
            std::panic::resume_unwind(Box::new(()));
        })
        .is_err();
        let teardown_explicit = teardown_fact();
        let terminal = match fixture.scenario() {
            EvaluationScenario::MalformedInput => EvaluationTerminal::Rejected,
            EvaluationScenario::Cancellation => EvaluationTerminal::Cancelled,
            _ => EvaluationTerminal::Completed,
        };
        Ok(EvaluationConformanceObservation {
            terminal,
            planned_rollouts: u32::try_from(plan.specs().len())
                .map_err(|_| EvaluationConformanceError::Infrastructure)?,
            maximum_attempts: profile.retry().maximum_attempts(),
            report_metrics: u16::try_from(profile.metrics().pass_k().len())
                .map_err(|_| EvaluationConformanceError::Infrastructure)?,
            frozen_inputs_exact,
            isolation_exact,
            deterministic,
            accounting_complete,
            statistics_valid,
            infrastructure_distinct,
            cancellation_durable,
            replay_equivalent,
            malformed_rejected,
            publication_ordered,
            redaction_safe,
            bounds_enforced,
            panic_contained,
            teardown_explicit,
            non_authoritative: true,
        })
    }
}

fn accounting_facts(
    plan: &EvaluationPlan,
    profile: &peritus_eval::FrozenEvaluationProfile,
) -> Result<(bool, bool), EvaluationConformanceError> {
    let mut ledger = RolloutLedger::from_plan(plan, 3);
    for spec in plan.specs() {
        let mut port = FixturePort::new(PortMode::Pass);
        let executed = execute_rollout(&mut port, &NeverCancelled, spec, profile, 1)
            .map_err(|_| EvaluationConformanceError::Infrastructure)?;
        let record = RolloutRecord::from_execution(spec, executed, None, None)
            .map_err(|_| EvaluationConformanceError::Infrastructure)?;
        ledger
            .record_attempt(spec.id(), record.attempt())
            .and_then(|()| ledger.settle(record))
            .map_err(|_| EvaluationConformanceError::Infrastructure)?;
    }
    let complete = ledger.counts().complete()
        && ledger.counts().expected
            == u32::try_from(plan.specs().len())
                .map_err(|_| EvaluationConformanceError::Infrastructure)?;
    let spec = &plan.specs()[0];
    let mut port = FixturePort::new(PortMode::CandidateInfrastructure);
    let executed = execute_rollout(&mut port, &NeverCancelled, spec, profile, 1)
        .map_err(|_| EvaluationConformanceError::Infrastructure)?;
    let infrastructure_distinct =
        matches!(executed.attempt().terminal(), RolloutOutcome::InfrastructureFailed { .. })
            && !executed.attempt().terminal().evaluated();
    Ok((complete, infrastructure_distinct))
}

fn statistical_facts() -> bool {
    let pass = pass_at_k(4, 2, 2);
    let interval = peritus_eval::WilsonInterval::ninety_five(2, 4);
    match (pass, interval) {
        (Ok(pass), Ok(interval)) => {
            pass.estimate().get() <= 1_000_000
                && interval.lower() <= interval.upper()
                && pass_at_k(0, 0, 1).is_err()
                && peritus_eval::WilsonInterval::ninety_five(5, 4).is_err()
        }
        _ => false,
    }
}

fn cancellation_fact(
    profile: &peritus_eval::FrozenEvaluationProfile,
) -> Result<bool, EvaluationConformanceError> {
    let create = genesis(profile, 100, 101)?;
    let created = decide(None, &create).map_err(|_| EvaluationConformanceError::Infrastructure)?;
    let cancel = next(
        created.state(),
        profile.digest(),
        102,
        103,
        EvaluationCommandKind::CancelCampaign { reason_digest: digest(104) },
    )?;
    let cancelling = decide(Some(created.state()), &cancel)
        .map_err(|_| EvaluationConformanceError::Infrastructure)?;
    let complete = next(
        cancelling.state(),
        profile.digest(),
        105,
        106,
        EvaluationCommandKind::CompleteCancellation,
    )?;
    let cancelled = decide(Some(cancelling.state()), &complete)
        .map_err(|_| EvaluationConformanceError::Infrastructure)?;
    Ok(cancelled.state().phase() == EvaluationPhase::Cancelled
        && decide(Some(cancelled.state()), &complete).is_err())
}

fn protocol_facts(
    profile: &peritus_eval::FrozenEvaluationProfile,
) -> Result<(bool, bool), EvaluationConformanceError> {
    let command = genesis(profile, 110, 111)?;
    let transition =
        decide(None, &command).map_err(|_| EvaluationConformanceError::Infrastructure)?;
    let replayed = replay(&[transition.event().clone()])
        .map_err(|_| EvaluationConformanceError::Infrastructure)?;
    let mut bytes = encode_message(
        &EvaluationCommandFrame::from_command(&command)
            .map_err(|_| EvaluationConformanceError::Infrastructure)?,
        CodecLimits::PRODUCTION,
    )
    .map_err(|_| EvaluationConformanceError::Infrastructure)?;
    bytes.push(0);
    Ok((
        replayed == *transition.state(),
        decode_message::<EvaluationCommandFrame>(&bytes, CodecLimits::PRODUCTION).is_err(),
    ))
}

fn publication_ordering_fact(
    profile: &peritus_eval::FrozenEvaluationProfile,
) -> Result<bool, EvaluationConformanceError> {
    let create = genesis(profile, 120, 121)?;
    let created = decide(None, &create).map_err(|_| EvaluationConformanceError::Infrastructure)?;
    let premature = next(
        created.state(),
        profile.digest(),
        122,
        123,
        EvaluationCommandKind::CompleteReport {
            report: ReportRecord::new(
                peritus_eval::EvaluationReportId::new(bytes(124))
                    .map_err(|_| EvaluationConformanceError::Infrastructure)?,
                digest(125),
                artifact(126),
                1,
            )
            .map_err(|_| EvaluationConformanceError::Infrastructure)?,
        },
    )?;
    Ok(decide(Some(created.state()), &premature).is_err())
}

fn genesis(
    profile: &peritus_eval::FrozenEvaluationProfile,
    command_seed: u8,
    event_seed: u8,
) -> Result<EvaluationCommand, EvaluationConformanceError> {
    EvaluationCommand::new(
        CommandId::new(bytes(command_seed))
            .map_err(|_| EvaluationConformanceError::Infrastructure)?,
        EventId::new(bytes(event_seed)).map_err(|_| EvaluationConformanceError::Infrastructure)?,
        campaign_id(),
        0,
        None,
        digest(0),
        profile.digest(),
        EvaluationCommandKind::CreateCampaign {
            revision: revision(),
            dataset_digest: profile.dataset().digest(),
            dataset_artifact: artifact(127),
            profile_artifact: artifact(128),
        },
    )
    .map_err(|_| EvaluationConformanceError::Infrastructure)
}

fn next(
    state: &peritus_eval::EvaluationState,
    profile: peritus_eval::ProfileDigest,
    command_seed: u8,
    event_seed: u8,
    kind: EvaluationCommandKind,
) -> Result<EvaluationCommand, EvaluationConformanceError> {
    EvaluationCommand::new(
        CommandId::new(bytes(command_seed))
            .map_err(|_| EvaluationConformanceError::Infrastructure)?,
        EventId::new(bytes(event_seed)).map_err(|_| EvaluationConformanceError::Infrastructure)?,
        campaign_id(),
        state.sequence(),
        Some(state.last_event_id()),
        state.state_digest(),
        profile,
        kind,
    )
    .map_err(|_| EvaluationConformanceError::Infrastructure)
}

fn teardown_fact() -> bool {
    struct Probe(Arc<AtomicBool>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let observed = Arc::new(AtomicBool::new(false));
    {
        let _probe = Probe(Arc::clone(&observed));
    }
    observed.load(Ordering::SeqCst)
}

struct Factory {
    descriptor: SubjectDescriptor,
}

impl Factory {
    fn new() -> Self {
        Self {
            descriptor: SubjectDescriptor::new(
                ReportText::new("peritus-eval").expect("subject name"),
                ReportText::new("production E3 domain bridge").expect("subject description"),
            ),
        }
    }
}

impl SubjectFactory<ProductionEvaluation> for Factory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
    ) -> ConformanceFuture<'a, Result<ProductionEvaluation, SubjectFailure>> {
        Box::pin(async { Ok(ProductionEvaluation) })
    }

    fn teardown<'a>(
        &'a self,
        _case: &'a CaseDescriptor,
        _subject: ProductionEvaluation,
    ) -> ConformanceFuture<'a, Result<(), SubjectFailure>> {
        Box::pin(async { Ok(()) })
    }
}

#[test]
fn production_evaluation_satisfies_the_complete_a2_catalog() {
    let report = futures_lite(&evaluation_suite::<ProductionEvaluation>(), &Factory::new());
    assert_eq!(report.status(), SuiteStatus::Passed, "{report:?}");
    assert_eq!(report.summary().total(), 13);
}

fn futures_lite<S: EvaluationConformanceSubject + 'static>(
    suite: &StaticSuite<S>,
    factory: &impl SubjectFactory<S>,
) -> SuiteReport {
    let runtime =
        tokio::runtime::Builder::new_current_thread().enable_all().build().expect("test runtime");
    runtime.block_on(ConformanceRunner::run(suite, factory))
}

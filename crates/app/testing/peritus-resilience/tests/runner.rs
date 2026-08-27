//! Fresh-subject execution, false-success rejection, and evidence-determinism integration tests.

use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::task::{Context, Poll, Waker};

use peritus_resilience::{
    AcceptanceObservation, ArtifactHealth, CancellationToken, CleanupObservation, CorruptTarget,
    CorruptionObservation, DependencyKind, DisruptionObservation, EvidenceAnchor, EvidenceDigest,
    EvidenceId, EvidenceKind, FaultInjection, JournalHealth, Milestone, MilestoneKind,
    OwnershipObservation, OwnershipResolution, PreparationObservation, ProjectionHealth,
    QualificationConfig, QualificationFuture, QualificationRunner, QualificationText,
    QualificationVerdict, RecoveredStateObservation, RecoveryAccounting, RecoveryObservation,
    ResilienceSubject, ResilienceSubjectFactory, ResourceUsage, RetryUsage, ScenarioCatalog,
    ScenarioSpec, SubjectDescriptor, SubjectError, SubjectId, TerminalState,
};

struct FakeSubject {
    cancellation: CancellationToken,
    false_success: bool,
}

impl ResilienceSubject for FakeSubject {
    fn prepare<'a>(
        &'a mut self,
        scenario: &'a ScenarioSpec,
    ) -> QualificationFuture<'a, Result<PreparationObservation, SubjectError>> {
        Box::pin(async move {
            assert!(!self.cancellation.is_cancelled());
            Ok(PreparationObservation::new(scenario.id().clone(), TerminalState::Active, digest(1)))
        })
    }

    fn inject<'a>(
        &'a mut self,
        scenario: &'a ScenarioSpec,
    ) -> QualificationFuture<'a, Result<DisruptionObservation, SubjectError>> {
        Box::pin(async move {
            Ok(DisruptionObservation::new(scenario.id().clone(), scenario.fault(), true))
        })
    }

    fn recover<'a>(
        &'a mut self,
        scenario: &'a ScenarioSpec,
    ) -> QualificationFuture<'a, Result<RecoveryObservation, SubjectError>> {
        Box::pin(async move { Ok(recovery(scenario, self.false_success)) })
    }
}

struct FakeFactory {
    descriptor: SubjectDescriptor,
    creates: AtomicUsize,
    cleanups: AtomicUsize,
    false_success: bool,
}

impl FakeFactory {
    fn new(false_success: bool) -> Self {
        Self {
            descriptor: SubjectDescriptor::new(
                SubjectId::new("peritus.test.release").expect("valid subject ID"),
                QualificationText::new("integrated test release").expect("valid text"),
                digest(9),
            ),
            creates: AtomicUsize::new(0),
            cleanups: AtomicUsize::new(0),
            false_success,
        }
    }
}

impl ResilienceSubjectFactory<FakeSubject> for FakeFactory {
    fn descriptor(&self) -> &SubjectDescriptor {
        &self.descriptor
    }

    fn create<'a>(
        &'a self,
        _scenario: &'a ScenarioSpec,
        cancellation: CancellationToken,
    ) -> QualificationFuture<'a, Result<FakeSubject, SubjectError>> {
        Box::pin(async move {
            self.creates.fetch_add(1, Ordering::Relaxed);
            Ok(FakeSubject { cancellation, false_success: self.false_success })
        })
    }

    fn cleanup<'a>(
        &'a self,
        _scenario: &'a ScenarioSpec,
        _subject: FakeSubject,
    ) -> QualificationFuture<'a, Result<CleanupObservation, SubjectError>> {
        Box::pin(async move {
            self.cleanups.fetch_add(1, Ordering::Relaxed);
            Ok(CleanupObservation::new(true, 0, 1))
        })
    }
}

#[test]
fn full_catalog_requires_and_cleans_up_one_fresh_subject_per_case() {
    let catalog = ScenarioCatalog::h1_production().expect("built-in catalog is valid");
    let factory = FakeFactory::new(false);
    let report = run_ready(QualificationRunner::run::<FakeSubject, _>(
        QualificationConfig::default(),
        &catalog,
        &factory,
    ));

    assert_eq!(report.verdict(), QualificationVerdict::Ready);
    assert_eq!(factory.creates.load(Ordering::Relaxed), catalog.scenarios().len());
    assert_eq!(factory.cleanups.load(Ordering::Relaxed), catalog.scenarios().len());
    assert_eq!(report.summary().passed(), catalog.scenarios().len());
}

#[test]
fn false_success_is_evidence_backed_not_ready() {
    let catalog = ScenarioCatalog::h1_production().expect("built-in catalog is valid");
    let factory = FakeFactory::new(true);
    let report = run_ready(QualificationRunner::run::<FakeSubject, _>(
        QualificationConfig::default(),
        &catalog,
        &factory,
    ));

    assert!(!report.is_ready());
    assert_eq!(report.summary().failed(), catalog.scenarios().len());
    assert_ne!(report.evidence_digest(), EvidenceDigest::from_bytes([0; 32]));
}

#[test]
fn identical_direct_observations_have_identical_report_digest() {
    let catalog = ScenarioCatalog::h1_production().expect("built-in catalog is valid");
    let first_factory = FakeFactory::new(false);
    let second_factory = FakeFactory::new(false);
    let first = run_ready(QualificationRunner::run::<FakeSubject, _>(
        QualificationConfig::default(),
        &catalog,
        &first_factory,
    ));
    let second = run_ready(QualificationRunner::run::<FakeSubject, _>(
        QualificationConfig::default(),
        &catalog,
        &second_factory,
    ));
    assert_eq!(first.evidence_digest(), second.evidence_digest());
}

fn recovery(scenario: &ScenarioSpec, false_success: bool) -> RecoveryObservation {
    let (journal, artifacts, projection, corruption) = match scenario.fault() {
        FaultInjection::Corruption(CorruptTarget::Journal) => (
            JournalHealth::HashDivergenceDetected,
            ArtifactHealth::Verified,
            ProjectionHealth::Verified,
            CorruptionObservation::new(Some(CorruptTarget::Journal), false),
        ),
        FaultInjection::Corruption(CorruptTarget::Projection) => (
            JournalHealth::Verified,
            ArtifactHealth::Verified,
            ProjectionHealth::RebuiltAndVerified,
            CorruptionObservation::new(Some(CorruptTarget::Projection), true),
        ),
        FaultInjection::Corruption(target) => (
            JournalHealth::Verified,
            ArtifactHealth::DivergenceDetected,
            ProjectionHealth::Verified,
            CorruptionObservation::new(Some(target), false),
        ),
        _ => (
            JournalHealth::RecoveredAndVerified,
            ArtifactHealth::Verified,
            ProjectionHealth::Verified,
            CorruptionObservation::new(None, true),
        ),
    };
    let exercises_owned_work = matches!(
        scenario.fault(),
        FaultInjection::DependencyDeath(_)
            | FaultInjection::DaemonKill(_)
            | FaultInjection::HostReboot(_)
    );
    let ownership = if exercises_owned_work {
        OwnershipObservation::new(true, 1, OwnershipResolution::new(0, 1, 0, 0), 1, 0)
    } else {
        OwnershipObservation::new(true, 0, OwnershipResolution::new(0, 0, 0, 0), 0, 0)
    };
    let retry = if let FaultInjection::RetryExhaustion(dependency) = scenario.fault() {
        let limits = QualificationConfig::default().retries();
        match dependency {
            DependencyKind::Provider => RetryUsage::new(limits.provider(), 0, 0, 1),
            DependencyKind::Tool => RetryUsage::new(0, limits.tool(), 0, 1),
            DependencyKind::Worker => RetryUsage::new(0, 0, limits.worker(), 1),
        }
    } else {
        RetryUsage::new(0, 0, 0, 1)
    };
    RecoveryObservation::new(
        scenario.id().clone(),
        scenario.expected_recovery(),
        RecoveredStateObservation::new(
            AcceptanceObservation::new(
                if false_success { TerminalState::Accepted } else { TerminalState::Failed },
                false_success,
                false_success,
            ),
            journal,
            artifacts,
            projection,
            corruption,
            0,
        ),
        RecoveryAccounting::new(ownership, retry, ResourceUsage::new(12, 4_096, 2, 1, 50)),
        evidence(),
        milestones(),
    )
    .expect("bounded canonical observation")
}

fn evidence() -> Vec<EvidenceAnchor> {
    EvidenceKind::REQUIRED
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            EvidenceAnchor::new(
                kind,
                EvidenceId::new(format!("evidence.{index}")).expect("valid evidence ID"),
                digest(u8::try_from(index).expect("required evidence count fits in u8")),
            )
        })
        .collect()
}

fn milestones() -> Vec<Milestone> {
    [
        MilestoneKind::Prepared,
        MilestoneKind::FaultArmed,
        MilestoneKind::FaultObserved,
        MilestoneKind::RecoveryStarted,
        MilestoneKind::Reconciled,
        MilestoneKind::Inspected,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, kind)| {
        Milestone::new(
            u16::try_from(index).expect("milestone count fits in u16"),
            kind,
            QualificationText::new(format!("milestone {index}")).expect("valid text"),
        )
    })
    .collect()
}

const fn digest(byte: u8) -> EvidenceDigest {
    EvidenceDigest::from_bytes([byte; 32])
}

fn run_ready<F: Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    match future.as_mut().poll(&mut context) {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("immediate test subject unexpectedly yielded"),
    }
}

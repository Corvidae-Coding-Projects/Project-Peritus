//! Commit/publish/ack/result recovery boundary classification.

#![allow(clippy::unwrap_used, reason = "fixed checked crash fixtures")]

use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};

use crate::runtime::{
    ChildProjectionPort, ChildReconciliation, DirectivePublisher, DirectiveReceipt,
    OrchestratorDriver, PendingDirectiveClass, classify_pending_directive,
};
use crate::{
    ChildAggregateKind, ChildHead, OrchestratorError, OrchestratorErrorKind,
    OrchestratorRecoveryAction, PendingDirective,
};

use super::support::{Scenario, bytes, pending_for_open_handoff};

#[test]
fn crash_after_commit_before_publish_recovers_the_same_deliverable_directive() {
    let source = Scenario::new();
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("driver-crash.sqlite3");
    let store = StoreId::new(bytes(850)).unwrap();
    let mut journal = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
    let mut driver = OrchestratorDriver::new();
    let mut publisher = RecordingPublisher { fail: false, calls: 0 };
    driver.step(&mut journal, &mut publisher, &source.steps()[0].0).unwrap();

    let mut planned = source.clone();
    let pending = pending_for_open_handoff(&planned);
    planned
        .apply_ok(crate::OrchestratorCommandKind::PublishDirective { directive: pending.clone() });
    publisher.fail = true;
    let error = driver
        .step(&mut journal, &mut publisher, &planned.steps()[1].0)
        .expect_err("publisher fails after the C0 commit");
    assert_eq!(error.kind(), OrchestratorErrorKind::External);
    assert_eq!(publisher.calls, 1);
    drop(journal);

    let journal = SqliteJournal::open(&path, store, SqliteJournalOptions::default()).unwrap();
    let recovered =
        OrchestratorDriver::recover(&journal, source.state().binding().run_id()).unwrap();
    assert_eq!(recovered.state().unwrap(), planned.state());
    let directive = recovered.state().unwrap().pending_directive().unwrap();
    let report = classify_pending_directive(
        directive,
        &mut FixedProjection { reconciliation: ChildReconciliation::Absent },
    )
    .unwrap();
    assert_eq!(report.directive_id(), pending.id());
    assert_eq!(report.class(), PendingDirectiveClass::Deliverable);
}

#[test]
fn publish_ack_and_child_result_boundaries_have_distinct_restart_actions() {
    let mut published = Scenario::new();
    let pending = pending_for_open_handoff(&published);
    published
        .apply_ok(crate::OrchestratorCommandKind::PublishDirective { directive: pending.clone() });
    let deliverable = classify_pending_directive(
        published.state().pending_directive().unwrap(),
        &mut FixedProjection { reconciliation: ChildReconciliation::Absent },
    )
    .unwrap();
    assert_eq!(deliverable.class(), PendingDirectiveClass::Deliverable);

    published.apply_ok(crate::OrchestratorCommandKind::AcknowledgeDirective {
        directive_id: pending.id(),
    });
    let active = classify_pending_directive(
        published.state().pending_directive().unwrap(),
        &mut FixedProjection { reconciliation: ChildReconciliation::Active },
    )
    .unwrap();
    assert_eq!(active.class(), PendingDirectiveClass::AcknowledgedAwaitingResult);

    let classification = crate::CancellationChildClassification::unreachable(
        ChildAggregateKind::Collaboration,
        published.state().current_candidate().revision(),
        super::support::digest(860),
    )
    .unwrap();
    let completed = classify_pending_directive(
        published.state().pending_directive().unwrap(),
        &mut FixedProjection { reconciliation: ChildReconciliation::Classified(classification) },
    )
    .unwrap();
    assert_eq!(completed.class(), PendingDirectiveClass::CompletedAwaitingObservation);
    assert!(completed.observation().is_some());
}

struct RecordingPublisher {
    fail: bool,
    calls: usize,
}

impl DirectivePublisher for RecordingPublisher {
    fn publish(
        &mut self,
        directive: &PendingDirective,
    ) -> Result<DirectiveReceipt, OrchestratorError> {
        self.calls += 1;
        if self.fail {
            Err(OrchestratorError::new(
                OrchestratorErrorKind::External,
                OrchestratorRecoveryAction::Replay,
                "fixture publisher interrupted",
            ))
        } else {
            Ok(DirectiveReceipt::new(directive.id(), directive.payload_digest()))
        }
    }
}

struct FixedProjection {
    reconciliation: ChildReconciliation,
}

impl ChildProjectionPort for FixedProjection {
    fn active_head(&mut self, _kind: ChildAggregateKind) -> Result<ChildHead, OrchestratorError> {
        Err(OrchestratorError::new(
            OrchestratorErrorKind::External,
            OrchestratorRecoveryAction::Replay,
            "active head is unused in this fixture",
        ))
    }

    fn reconcile(
        &mut self,
        _directive: &PendingDirective,
    ) -> Result<ChildReconciliation, OrchestratorError> {
        Ok(self.reconciliation.clone())
    }
}

use crate::{
    AggregateKind, AppendRequest, HeadExpectation, JournalErrorKind, OutboxDraft, OutboxId,
};
use peritus_types::Sha256Digest;
use tempfile::TempDir;

use super::{command, draft, event, key, open, store_id};

#[test]
fn stale_fence_cannot_acknowledge_a_reclaimed_outbox_row() {
    let temp = TempDir::new().expect("temporary directory");
    let mut first_worker = open(&temp);
    let aggregate = key(AggregateKind::Kernel, 70);
    let outbox_id = OutboxId::new([70; 16]).expect("outbox identity");
    let append = AppendRequest::new(
        store_id(),
        command(70),
        Sha256Digest::new([70; 32]),
        vec![HeadExpectation::Absent(aggregate)],
        vec![draft(aggregate, 1, event(70), None, 70)],
        Vec::new(),
        Vec::new(),
        None,
        None,
        vec![OutboxDraft::new(outbox_id, "race.target".into(), vec![70], 3).expect("outbox draft")],
    )
    .plan()
    .expect("append plan");
    first_worker.append(append).expect("append outbox row");

    let first_claim = first_worker.claim_outbox(10, 20).expect("first claim").expect("row");
    let first_fence = first_claim.fence().expect("first fence");

    let mut second_worker = open(&temp);
    let reclaimed = second_worker.claim_outbox(20, 30).expect("reclaim").expect("row");
    let reclaimed_fence = reclaimed.fence().expect("reclaimed fence");
    assert!(reclaimed_fence > first_fence);
    assert_eq!(reclaimed.attempts(), 2);

    assert_eq!(
        first_worker
            .acknowledge_outbox(outbox_id, first_fence)
            .expect_err("stale fence must not acknowledge reclaimed row")
            .kind(),
        JournalErrorKind::StaleHead
    );
    second_worker
        .acknowledge_outbox(outbox_id, reclaimed_fence)
        .expect("current fence acknowledges");
    first_worker
        .acknowledge_outbox(outbox_id, first_fence)
        .expect("an already acknowledged row is idempotent");

    let missing = OutboxId::new([71; 16]).expect("missing identity");
    assert_eq!(
        first_worker
            .acknowledge_outbox(missing, 1)
            .expect_err("unknown row remains not found")
            .kind(),
        JournalErrorKind::NotFound
    );
}

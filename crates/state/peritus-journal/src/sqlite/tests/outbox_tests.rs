use crate::{
    AggregateKind, AppendRequest, HeadExpectation, JournalErrorKind, OutboxAcknowledgement,
    OutboxDraft, OutboxId,
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

#[test]
fn aggregate_append_atomically_acknowledges_exact_claimed_directive() {
    let temp = TempDir::new().expect("temporary directory");
    let mut journal = open(&temp);
    let aggregate = key(AggregateKind::Harness, 80);
    let first_event = event(80);
    let outbox_id = OutboxId::new([80; 16]).expect("outbox identity");
    let planned = AppendRequest::new(
        store_id(),
        command(80),
        Sha256Digest::new([80; 32]),
        vec![HeadExpectation::Absent(aggregate)],
        vec![draft(aggregate, 1, first_event, None, 80)],
        Vec::new(),
        Vec::new(),
        None,
        None,
        vec![
            OutboxDraft::new(outbox_id, "harness.materialize".into(), vec![80], 3)
                .expect("outbox draft"),
        ],
    )
    .plan()
    .expect("planning append");
    journal.append(planned).expect("commit planning directive");
    let claim = journal.claim_outbox(10, 20).expect("claim").expect("directive");
    let fence = claim.fence().expect("claim fence");
    let head = journal.head(aggregate).expect("head query").expect("head");

    let stale = AppendRequest::new(
        store_id(),
        command(81),
        Sha256Digest::new([81; 32]),
        vec![HeadExpectation::Present(head)],
        vec![draft(aggregate, 2, event(81), Some(first_event), 81)],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .with_outbox_acknowledgements(vec![
        OutboxAcknowledgement::new(outbox_id, fence + 1).expect("stale acknowledgement"),
    ])
    .expect("bind stale acknowledgement")
    .plan()
    .expect("stale append plan");
    assert_eq!(
        journal.append(stale).expect_err("stale fence rejects whole append").kind(),
        JournalErrorKind::StaleHead
    );
    assert_eq!(journal.head(aggregate).expect("head query").expect("head").sequence().get(), 1);

    let completed = AppendRequest::new(
        store_id(),
        command(82),
        Sha256Digest::new([82; 32]),
        vec![HeadExpectation::Present(head)],
        vec![draft(aggregate, 2, event(82), Some(first_event), 82)],
        Vec::new(),
        Vec::new(),
        None,
        None,
        Vec::new(),
    )
    .with_outbox_acknowledgements(vec![
        OutboxAcknowledgement::new(outbox_id, fence).expect("acknowledgement"),
    ])
    .expect("bind acknowledgement")
    .plan()
    .expect("completion append plan");
    journal.append(completed).expect("commit completion and acknowledgement");
    assert_eq!(journal.head(aggregate).expect("head query").expect("head").sequence().get(), 2);
    assert!(journal.claim_outbox(20, 30).expect("claim query").is_none());
}

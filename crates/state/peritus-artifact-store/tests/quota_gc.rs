//! Checked quota and deterministic durable collection tests.

mod support;

use peritus_artifact_store::{
    ArtifactReferenceSet, CollectionGeneration, ErrorCode, GcAction, GcInventoryEntry, GcPlan,
    QuarantineState, QuotaPlan, QuotaSnapshot, ReferenceOwner, ReferenceRoots,
};
use peritus_types::Sha256Digest;

use support::{digest, request, store};

#[test]
fn quota_plans_use_checked_arithmetic_and_enforce_exact_limit() {
    let snapshot = QuotaSnapshot::new(7, 2, 10).expect("valid snapshot");
    let exact = QuotaPlan::reserve(snapshot, 1).expect("exact quota limit");
    assert_eq!(exact.reserved_after(), 3);
    assert_eq!(exact.total_after(), 10);
    assert_eq!(
        QuotaPlan::reserve(snapshot, 2).expect_err("over quota").code(),
        ErrorCode::QuotaExceeded,
    );
    assert_eq!(
        QuotaSnapshot::new(u64::MAX, 1, u64::MAX).expect_err("overflow").code(),
        ErrorCode::ArithmeticOverflow,
    );
    let overflowing_reserved = QuotaSnapshot::new(0, u64::MAX, u64::MAX).expect("representable");
    assert_eq!(
        QuotaPlan::reserve(overflowing_reserved, 1).expect_err("reservation overflow").code(),
        ErrorCode::ArithmeticOverflow,
    );
}

#[test]
fn writer_admission_uses_durable_catalog_quota() {
    let (_directory, store) = store(10, 10);
    let first = b"12345678";
    let mut writer = store.begin_write(request(first, 10, 1)).expect("first writer");
    writer.write_chunk(first).expect("first bytes");
    writer.finalize().expect("first artifact");
    assert_eq!(store.quota_snapshot(0).expect("durable quota").used_bytes(), 8);

    let Err(error) = store.begin_write(request(b"abc", 10, 2)) else {
        panic!("durable quota must reject the additional artifact");
    };
    assert_eq!(error.code(), ErrorCode::QuotaExceeded);
}

#[test]
fn durable_references_drive_quarantine_then_later_sweep() {
    let (directory, mut store) = store(128, 512);
    let kept = b"referenced";
    let garbage = b"collect me";
    for (bytes, event) in [(kept.as_slice(), 1), (garbage.as_slice(), 2)] {
        let mut writer = store.begin_write(request(bytes, 32, event)).expect("writer");
        writer.write_chunk(bytes).expect("bytes");
        writer.finalize().expect("finalize");
    }
    let owner = ReferenceOwner::journal(Sha256Digest::new([7; 32]));
    store.add_reference(owner, digest(kept)).expect("durable reference");

    let first_generation = CollectionGeneration::new(1).expect("generation");
    let first = store.plan_gc(first_generation).expect("first plan");
    assert_eq!(first.actions().len(), 1);
    assert!(
        matches!(first.actions()[0], GcAction::Quarantine { digest: value, .. } if value == digest(garbage))
    );
    let applied = store.apply_gc_plan(&first).expect("quarantine applies");
    assert_eq!(applied.quarantined(), 1);
    assert!(matches!(
        store.metadata(digest(garbage)).expect("query").expect("record").quarantine(),
        QuarantineState::Quarantined { .. },
    ));

    assert!(store.plan_gc(first_generation).expect("same generation plan").actions().is_empty());
    let second =
        store.plan_gc(CollectionGeneration::new(2).expect("generation")).expect("sweep plan");
    assert!(
        matches!(second.actions(), [GcAction::Delete { digest: value, .. }] if *value == digest(garbage))
    );
    let swept = store.apply_gc_plan(&second).expect("sweep applies");
    assert_eq!(swept.deleted(), 1);
    assert_eq!(swept.deleted_bytes(), garbage.len() as u64);
    assert!(store.metadata(digest(garbage)).expect("query").is_none());
    assert!(store.verify(digest(kept)).is_ok());

    drop(store);
    let reopened = peritus_artifact_store::ArtifactStore::open(
        peritus_artifact_store::StoreConfig::new(directory.path(), 128, 512).expect("config"),
    )
    .expect("restart");
    assert!(reopened.reference_roots().expect("roots").journal().contains(&digest(kept)));
}

#[test]
fn pure_gc_plan_is_canonical_and_restores_marked_quarantine() {
    let first = digest(b"first");
    let second = digest(b"second");
    let generation = CollectionGeneration::new(3).expect("generation");
    let since = CollectionGeneration::new(1).expect("generation");
    let mut journal = ArtifactReferenceSet::new();
    journal.insert(second);
    let roots = ReferenceRoots::new(journal, ArtifactReferenceSet::new());
    let entries = [
        GcInventoryEntry::new(second, 6, QuarantineState::Quarantined { since }),
        GcInventoryEntry::new(first, 5, QuarantineState::Active),
    ];
    let plan = GcPlan::build(generation, entries, &roots).expect("plan");
    let mut action_digests: Vec<_> = plan.actions().iter().copied().map(GcAction::digest).collect();
    let mut sorted = action_digests.clone();
    sorted.sort_unstable();
    assert_eq!(action_digests, sorted);
    assert!(
        plan.actions()
            .iter()
            .any(|action| matches!(action, GcAction::Restore { digest, .. } if *digest == second))
    );
    assert!(
        plan.actions().iter().any(
            |action| matches!(action, GcAction::Quarantine { digest, .. } if *digest == first)
        )
    );

    action_digests.clear();
    assert_eq!(
        GcPlan::build(generation, [entries[0], entries[0]], &roots)
            .expect_err("duplicates rejected")
            .code(),
        ErrorCode::InvalidCollectionPlan,
    );
}

#[test]
fn rewriting_identical_quarantined_content_restores_it_atomically() {
    let (directory, mut store) = store(64, 128);
    let bytes = b"needed again";
    let content_digest = digest(bytes);
    let mut writer = store.begin_write(request(bytes, 32, 1)).expect("writer");
    writer.write_chunk(bytes).expect("bytes");
    writer.finalize().expect("finalize");
    let plan =
        store.plan_gc(CollectionGeneration::new(1).expect("generation")).expect("quarantine plan");
    store.apply_gc_plan(&plan).expect("quarantine");

    let mut rewrite = store.begin_write(request(bytes, 32, 2)).expect("rewrite writer");
    rewrite.write_chunk(bytes).expect("rewrite bytes");
    rewrite.finalize().expect("rewrite restores");
    assert!(store.verify(content_digest).is_ok());
    assert!(!support::quarantine_path(directory.path(), content_digest).exists());
}

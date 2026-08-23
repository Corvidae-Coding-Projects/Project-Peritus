//! Deterministic portable bundle assembly and offline verification integration tests.

mod support;

use peritus_evidence::{BundleLimits, EvidenceErrorKind, assemble_bundle, verify_bundle};
use support::{Fixture, revision};

#[test]
fn deterministic_streaming_bundle_reverifies_offline_and_after_restart() {
    let mut fixture = Fixture::new();
    let revision = revision();
    let artifact = fixture.finalize(b"portable evidence artifact bytes");
    let position = fixture.append(&revision, Some(artifact));
    let export = fixture.export();
    let draft = Fixture::draft(60, revision, position, vec![artifact], Vec::new());
    let mut store = fixture.evidence_store();
    let record = store.admit(draft, &export, &fixture.artifacts).expect("admit evidence");
    let limits = BundleLimits::default();

    let plan = store
        .plan_bundle(&[record.id()], &revision, &export, &fixture.artifacts, limits)
        .expect("plan bundle");
    let mut first = Vec::new();
    let first_receipt =
        assemble_bundle(&plan, &fixture.artifacts, &mut first, limits).expect("assemble bundle");
    let mut second = Vec::new();
    let second_receipt =
        assemble_bundle(&plan, &fixture.artifacts, &mut second, limits).expect("repeat bundle");
    assert_eq!(first, second);
    assert_eq!(first_receipt, second_receipt);

    let verified = verify_bundle(first.as_slice(), limits).expect("offline verification");
    assert_eq!(verified.manifest(), plan.manifest());
    assert_eq!(verified.bundle_digest(), first_receipt.bundle_digest());
    assert_eq!(verified.byte_count(), first_receipt.byte_count());

    drop(store);
    let reopened = support::open_evidence(&fixture.path);
    let restarted_plan = reopened
        .plan_bundle(&[record.id()], &revision, &export, &fixture.artifacts, limits)
        .expect("plan after restart");
    let mut restarted = Vec::new();
    assemble_bundle(&restarted_plan, &fixture.artifacts, &mut restarted, limits)
        .expect("assemble after restart");
    assert_eq!(restarted, first);
}

#[test]
fn offline_verification_rejects_corruption_truncation_and_trailing_bytes() {
    let mut fixture = Fixture::new();
    let revision = revision();
    let position = fixture.append(&revision, None);
    let export = fixture.export();
    let draft = Fixture::draft(61, revision, position, Vec::new(), Vec::new());
    let mut store = fixture.evidence_store();
    let record = store.admit(draft, &export, &fixture.artifacts).expect("admit evidence");
    let limits = BundleLimits::default();
    let plan = store
        .plan_bundle(&[record.id()], &revision, &export, &fixture.artifacts, limits)
        .expect("plan bundle");
    let mut bytes = Vec::new();
    assemble_bundle(&plan, &fixture.artifacts, &mut bytes, limits).expect("assemble bundle");

    let mut corrupt = bytes.clone();
    let middle = corrupt.len() / 2;
    corrupt[middle] ^= 1;
    assert_eq!(
        verify_bundle(corrupt.as_slice(), limits).expect_err("corruption rejected").kind(),
        EvidenceErrorKind::InvalidBundle
    );
    assert!(verify_bundle(&bytes[..bytes.len() - 1], limits).is_err());
    bytes.push(0);
    assert_eq!(
        verify_bundle(bytes.as_slice(), limits).expect_err("trailing byte rejected").kind(),
        EvidenceErrorKind::InvalidBundle
    );
}

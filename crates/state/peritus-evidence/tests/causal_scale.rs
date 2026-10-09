//! Public library scale boundaries and immutable history across revisions.

mod support;
use peritus_evidence::{
    BundleLimits, EvidenceDraft, EvidenceErrorKind, EvidenceId, EvidenceKind, EvidenceRecord,
    EvidenceSource, assemble_bundle, verify_bundle,
};
use peritus_types::Sha256Digest;
use std::io::{Seek, SeekFrom};
use support::{Fixture, make_revision, revision};

fn identity(value: u128) -> EvidenceId {
    EvidenceId::new(value.to_be_bytes()).expect("nonzero id")
}

#[test]
fn more_than_4096_parents_and_bundle_entries_preserve_the_original_revisions() {
    let mut fixture = Fixture::new();
    let historical = revision();
    let current = make_revision([2, 3, 4, 1, 2, 5, 6]);
    let parent_position = fixture.append(&historical, None);
    let child_position = fixture.append(&current, None);
    let export = fixture.export();
    let mut store = fixture.evidence_store();
    let mut parents = Vec::new();
    let mut parent_digests = Vec::new();
    for value in 1_u128..=4097 {
        let draft = EvidenceDraft::new(
            identity(value),
            EvidenceKind::new("execution-result").expect("kind"),
            EvidenceSource::new("local-runner").expect("source"),
            historical,
            parent_position,
            Sha256Digest::new([9; 32]),
            vec![],
            vec![],
        )
        .expect("parent draft");
        let parent = store.admit(draft, &export, &fixture.artifacts).expect("parent admission");
        assert!(
            parent
                .canonical_bytes()
                .windows(b"peritus-evidence-record-v1".len())
                .any(|window| window == b"peritus-evidence-record-v1")
        );
        parents.push(parent.id());
        parent_digests.push(parent.record_digest());
    }
    let draft = EvidenceDraft::new(
        identity(5000),
        EvidenceKind::new("causal-result").expect("kind"),
        EvidenceSource::new("local-runner").expect("source"),
        current,
        child_position,
        Sha256Digest::new([10; 32]),
        vec![],
        parents.clone(),
    )
    .expect("4097 causes are legitimate");
    let child =
        store.admit(draft, &export, &fixture.artifacts).expect("wide causal record admission");
    assert_eq!(
        EvidenceRecord::verify_portable(&child.canonical_bytes()).expect("wide portable record"),
        child
    );
    let plan = store
        .plan_bundle(&[child.id()], &current, &export, &fixture.artifacts, BundleLimits::default())
        .expect("historical closure automatically included");
    assert_eq!(plan.records().len(), 4098);
    assert_eq!(plan.manifest().authority_records(), &[child.id()]);
    for (record, (id, digest)) in plan.records().iter().zip(parents.iter().zip(&parent_digests)) {
        assert_eq!(record.id(), *id);
        assert_eq!(record.record_digest(), *digest);
        assert_eq!(record.revision(), &historical);
        assert!(!plan.manifest().is_authority(record.id()));
    }
    assert_eq!(plan.records().last(), Some(&child));
    assert_eq!(
        store
            .plan_bundle(
                &[parents[0]],
                &current,
                &export,
                &fixture.artifacts,
                BundleLimits::default()
            )
            .expect_err("historical record cannot assert present authority")
            .kind(),
        EvidenceErrorKind::StaleEvidence
    );
    assert!(
        store
            .plan_bundle(
                &[child.id()],
                &current,
                &export,
                &fixture.artifacts,
                BundleLimits::optional(Some(4096), None, None).expect("explicit budget")
            )
            .is_err()
    );
    let mut bytes = Vec::new();
    let receipt = assemble_bundle(&plan, &fixture.artifacts, &mut bytes, BundleLimits::default())
        .expect("wide closure assembly");
    let verified = verify_bundle(bytes.as_slice(), BundleLimits::default())
        .expect("wide closure offline verification");
    assert_eq!(verified.manifest(), plan.manifest());
    assert_eq!(verified.byte_count(), receipt.byte_count());
    drop(store);
    let reopened = fixture.evidence_store();
    assert_eq!(reopened.load(child.id()).expect("reopen wide record"), Some(child));
}

#[test]
fn metadata_over_32_mib_is_durable_and_portable_without_a_hidden_ceiling() {
    let mut fixture = Fixture::new();
    let rev = revision();
    let position = fixture.append(&rev, None);
    let export = fixture.export();
    let mut store = fixture.evidence_store();
    let kind =
        EvidenceKind::new("a".repeat(33 * 1024 * 1024)).expect("long canonical semantic tag");
    let draft = EvidenceDraft::new(
        identity(7000),
        kind,
        EvidenceSource::new("scale-consumer").expect("source"),
        rev,
        position,
        Sha256Digest::new([23; 32]),
        vec![],
        vec![],
    )
    .expect("large metadata draft");
    let record = store
        .admit(draft, &export, &fixture.artifacts)
        .expect("SQLite stores metadata above old limit");
    let plan = store
        .plan_bundle(&[record.id()], &rev, &export, &fixture.artifacts, BundleLimits::default())
        .expect("large metadata plan");
    let file = tempfile::tempfile_in(fixture.temp.path()).expect("bundle file");
    let receipt = assemble_bundle(&plan, &fixture.artifacts, &file, BundleLimits::default())
        .expect("large metadata assembly");
    assert!(receipt.byte_count() > 32 * 1024 * 1024);
    let mut file = file;
    file.seek(SeekFrom::Start(0)).expect("rewind");
    assert_eq!(
        verify_bundle(&file, BundleLimits::default())
            .expect("large metadata offline verification")
            .bundle_digest(),
        receipt.bundle_digest()
    );
    drop(store);
    let reopened = fixture.evidence_store();
    assert_eq!(
        reopened
            .load(record.id())
            .expect("large metadata restart")
            .expect("record retained")
            .record_digest(),
        record.record_digest()
    );
}

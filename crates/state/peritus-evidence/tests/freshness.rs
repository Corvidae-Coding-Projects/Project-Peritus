//! Exact revision drift and durable invalidation integration tests.

mod support;

use peritus_evidence::{EvidenceInvalidation, Freshness, RevisionDrift, evaluate_freshness};
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};
use support::{Fixture, make_revision, revision};

#[test]
fn revision_digest_matches_the_journal_canonical_tuple_encoding() {
    let revision = revision();
    let mut bytes = Vec::with_capacity(112);
    bytes.extend_from_slice(revision.acceptance_spec_id().as_bytes());
    bytes.extend_from_slice(revision.harness_id().as_bytes());
    bytes.extend_from_slice(revision.workspace_id().as_bytes());
    bytes.extend_from_slice(&revision.workspace_generation().get().to_be_bytes());
    bytes.extend_from_slice(&revision.workspace_revision().get().to_be_bytes());
    bytes.extend_from_slice(revision.policy_id().as_bytes());
    bytes.extend_from_slice(revision.provider_profile_id().as_bytes());
    assert_eq!(
        peritus_evidence::revision_digest(&revision),
        Sha256Digest::new(Sha256::digest(bytes).into())
    );
}

#[test]
fn every_revision_tuple_component_drifts_independently() {
    let mut fixture = Fixture::new();
    let stored_revision = revision();
    let position = fixture.append(&stored_revision, None);
    let export = fixture.export();
    let draft = Fixture::draft(50, stored_revision, position, Vec::new(), Vec::new());
    let mut store = fixture.evidence_store();
    let record = store.admit(draft, &export, &fixture.artifacts).expect("admit evidence");

    let cases = [
        ([8, 3, 4, 1, 1, 5, 6], RevisionDrift::AcceptanceSpec),
        ([2, 8, 4, 1, 1, 5, 6], RevisionDrift::Harness),
        ([2, 3, 8, 1, 1, 5, 6], RevisionDrift::Workspace),
        ([2, 3, 4, 8, 1, 5, 6], RevisionDrift::WorkspaceGeneration),
        ([2, 3, 4, 1, 8, 5, 6], RevisionDrift::WorkspaceRevision),
        ([2, 3, 4, 1, 1, 8, 6], RevisionDrift::Policy),
        ([2, 3, 4, 1, 1, 5, 8], RevisionDrift::ProviderProfile),
    ];
    for (parts, expected) in cases {
        let current = make_revision(parts);
        assert_eq!(evaluate_freshness(&record, &current, None), Freshness::RevisionStale(expected));
        assert_eq!(
            store.freshness(record.id(), &current).expect("durable freshness"),
            Freshness::RevisionStale(expected)
        );
    }
}

#[test]
fn explicit_later_invalidation_is_durable_and_dominates_revision_freshness() {
    let mut fixture = Fixture::new();
    let revision = revision();
    let position = fixture.append(&revision, None);
    let first_export = fixture.export();
    let draft = Fixture::draft(51, revision, position, Vec::new(), Vec::new());
    let mut store = fixture.evidence_store();
    let record = store.admit(draft, &first_export, &fixture.artifacts).expect("admit evidence");

    let invalidating_position = fixture.append(&revision, None);
    let export = fixture.export();
    let event = &export.records()[1];
    let invalidation = EvidenceInvalidation::new(
        record.id(),
        invalidating_position,
        event.event_id(),
        event.event_hash(),
        Sha256Digest::new([77; 32]),
    )
    .expect("invalidation");
    store.invalidate(invalidation, &export).expect("durable invalidation");
    assert_eq!(
        store.freshness(record.id(), &revision).expect("freshness"),
        Freshness::Invalidated(invalidation)
    );

    drop(store);
    let reopened = support::open_evidence(&fixture.path);
    assert_eq!(
        reopened.freshness(record.id(), &revision).expect("restart freshness"),
        Freshness::Invalidated(invalidation)
    );
}

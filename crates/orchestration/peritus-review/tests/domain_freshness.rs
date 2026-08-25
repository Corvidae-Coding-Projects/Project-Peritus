//! Exact revision-component freshness tests.

use peritus_review::evidence_is_fresh;
use peritus_types::{
    AcceptanceSpecId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, WorkspaceId,
};

#[test]
fn every_revision_tuple_component_is_an_independent_freshness_fence() {
    let original = revision([1, 2, 3, 1, 1, 4, 5]);
    assert!(evidence_is_fresh(original, original));
    for changed in [
        revision([9, 2, 3, 1, 1, 4, 5]),
        revision([1, 9, 3, 1, 1, 4, 5]),
        revision([1, 2, 9, 1, 1, 4, 5]),
        revision([1, 2, 3, 9, 1, 4, 5]),
        revision([1, 2, 3, 1, 9, 4, 5]),
        revision([1, 2, 3, 1, 1, 9, 5]),
        revision([1, 2, 3, 1, 1, 4, 9]),
    ] {
        assert!(!evidence_is_fresh(changed, original));
    }
}

fn revision(parts: [u8; 7]) -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new([parts[0]; 16]).expect("acceptance id"),
        HarnessId::new([parts[1]; 16]).expect("harness id"),
        WorkspaceId::new([parts[2]; 16]).expect("workspace id"),
        Generation::new(u64::from(parts[3])).expect("generation"),
        RevisionNumber::new(u64::from(parts[4])).expect("revision"),
        PolicyId::new([parts[5]; 16]).expect("policy id"),
        ProviderProfileId::new([parts[6]; 16]).expect("provider profile id"),
    )
}

//! Exact revision-tuple boundary behavior.

use peritus_types::{
    AcceptanceSpecId, Generation, HarnessId, PolicyId, ProviderProfileId, RevisionNumber,
    RevisionTuple, WorkspaceId,
};

const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}

fn tuple(parts: [u8; 7]) -> RevisionTuple {
    RevisionTuple::new(
        AcceptanceSpecId::new(bytes(parts[0])).expect("acceptance spec"),
        HarnessId::new(bytes(parts[1])).expect("harness"),
        WorkspaceId::new(bytes(parts[2])).expect("workspace"),
        Generation::new(u64::from(parts[3])).expect("generation"),
        RevisionNumber::new(u64::from(parts[4])).expect("revision"),
        PolicyId::new(bytes(parts[5])).expect("policy"),
        ProviderProfileId::new(bytes(parts[6])).expect("provider profile"),
    )
}

#[test]
fn revision_tuple_preserves_every_nominal_component() {
    let tuple = tuple([1, 2, 3, 4, 5, 6, 7]);

    assert_eq!(tuple.acceptance_spec_id().into_bytes(), bytes(1));
    assert_eq!(tuple.harness_id().into_bytes(), bytes(2));
    assert_eq!(tuple.workspace_id().into_bytes(), bytes(3));
    assert_eq!(tuple.workspace_generation().get(), 4);
    assert_eq!(tuple.workspace_revision().get(), 5);
    assert_eq!(tuple.policy_id().into_bytes(), bytes(6));
    assert_eq!(tuple.provider_profile_id().into_bytes(), bytes(7));
}

#[test]
fn changing_any_component_changes_tuple_identity() {
    let original = tuple([1, 2, 3, 4, 5, 6, 7]);
    let changed = [
        [9, 2, 3, 4, 5, 6, 7],
        [1, 9, 3, 4, 5, 6, 7],
        [1, 2, 9, 4, 5, 6, 7],
        [1, 2, 3, 9, 5, 6, 7],
        [1, 2, 3, 4, 9, 6, 7],
        [1, 2, 3, 4, 5, 9, 7],
        [1, 2, 3, 4, 5, 6, 9],
    ];

    for candidate in changed.map(tuple) {
        assert_ne!(original, candidate);
    }
}

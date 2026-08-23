//! Exact credential-registry snapshot digest binding evidence.

mod support;

use peritus_approval::{ApprovalChoice, CredentialStatus, verify_signed_decision};
use peritus_policy::{ActorRole, AuthorityTier};
use peritus_types::{ActorId, Generation, RevisionNumber};

#[test]
fn canonical_registry_digest_is_deterministic_and_field_sensitive() {
    let ids = support::ids();
    let build = |actor, status, generation, revision| {
        support::credential_registry(
            actor,
            ActorRole::HumanAuthority,
            vec![ActorRole::HumanAuthority],
            AuthorityTier::Organization,
            status,
            support::window(10, 80),
            generation,
            revision,
        )
    };
    let first = build(
        ids.responder,
        CredentialStatus::Enabled,
        Generation::first(),
        RevisionNumber::first(),
    );
    let same = build(
        ids.responder,
        CredentialStatus::Enabled,
        Generation::first(),
        RevisionNumber::first(),
    );
    assert_eq!(
        first.canonical_bytes().expect("canonical bytes"),
        same.canonical_bytes().expect("canonical bytes")
    );
    assert_eq!(first.digest().expect("digest"), same.digest().expect("digest"));

    let changed = [
        build(
            ActorId::new([0x31; 16]).expect("other actor"),
            CredentialStatus::Enabled,
            Generation::first(),
            RevisionNumber::first(),
        ),
        build(
            ids.responder,
            CredentialStatus::Disabled,
            Generation::first(),
            RevisionNumber::first(),
        ),
        build(
            ids.responder,
            CredentialStatus::Enabled,
            Generation::new(2).expect("next generation"),
            RevisionNumber::first(),
        ),
        build(
            ids.responder,
            CredentialStatus::Enabled,
            Generation::first(),
            RevisionNumber::new(2).expect("next revision"),
        ),
    ];
    let original = first.digest().expect("original digest");
    for registry in changed {
        assert_ne!(registry.digest().expect("changed digest"), original);
    }
}

#[test]
fn authenticated_observation_retains_the_exact_snapshot_digest() {
    let fixture = support::signed_fixture(ApprovalChoice::ApproveOnce);
    let expected = fixture.registry.digest().expect("registry digest");
    let observation = verify_signed_decision(
        &fixture.request,
        &fixture.signed,
        &fixture.registry,
        fixture.observed_at,
    )
    .expect("authenticated decision");
    assert_eq!(observation.registry_digest(), expected);
}

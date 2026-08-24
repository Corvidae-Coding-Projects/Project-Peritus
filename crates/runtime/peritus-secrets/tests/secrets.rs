//! Secret material, store, lease, delivery, redaction, recovery, and cleanup tests.

#![cfg(feature = "test-memory-store")]

use std::sync::Arc;

use peritus_sandbox::{
    BrokeredHandleLabel, EnvironmentName, SandboxPath, SecretDelivery, SecretGrant, SecretReference,
};
use peritus_secrets::{
    CredentialStore, MemoryCredentialStore, MemoryStoreOutcome, RedactionSet,
    SecretDeliveryContext, SecretDeliverySession, SecretErrorKind, SecretLease, SecretLeaseId,
    SecretLeaseState, SecretMaterial, SecretPreparation, SecretRecoveryRecord, SecretRecoveryState,
};
use peritus_types::{EnvironmentId, ProcessId, ResourceId, Sha256Digest};

#[test]
fn material_store_outcomes_and_versions_are_exact_without_debug_disclosure() {
    let canary = b"CANARY-super-secret-42";
    let reference = reference(canary);
    let material = SecretMaterial::new(canary.to_vec()).unwrap();
    let debug = format!("{material:?}");
    assert!(!debug.contains("CANARY"));
    assert!(debug.contains("REDACTED"));

    let store = MemoryCredentialStore::new();
    store.insert(reference, canary.to_vec());
    assert!(!format!("{store:?}").contains("CANARY"));
    assert_eq!(store.lookup(reference).unwrap().len(), canary.len());
    for (outcome, expected) in [
        (MemoryStoreOutcome::Locked, SecretErrorKind::Locked),
        (MemoryStoreOutcome::Denied, SecretErrorKind::Denied),
        (MemoryStoreOutcome::Unavailable, SecretErrorKind::Unavailable),
        (MemoryStoreOutcome::Corrupt, SecretErrorKind::Corrupt),
        (MemoryStoreOutcome::Io, SecretErrorKind::Io),
    ] {
        store.set_outcome(outcome);
        assert_eq!(store.lookup(reference).unwrap_err().kind(), expected);
    }
    store.set_outcome(MemoryStoreOutcome::Available);
    let stale = SecretReference::new(reference.resource_id(), Sha256Digest::new([99; 32]));
    store.insert(stale, canary.to_vec());
    assert_eq!(store.lookup(stale).unwrap_err().kind(), SecretErrorKind::StaleVersion);
}

#[test]
fn leases_reject_drift_expiry_reuse_and_revocation() {
    let reference = reference(b"lease-value");
    let delivery = SecretDelivery::Environment(EnvironmentName::new("TOKEN").unwrap());
    let mut active = lease(1, reference, delivery.clone(), 2, 1_000);
    assert!(
        active
            .consume(owner(), environment(), digest(3), digest(4), reference, &delivery, 999)
            .is_ok()
    );
    assert_eq!(active.remaining_uses(), 1);
    assert!(
        active
            .consume(owner(), environment(), digest(3), digest(44), reference, &delivery, 999)
            .is_err()
    );
    assert!(
        active
            .consume(owner(), environment(), digest(3), digest(4), reference, &delivery, 1_000)
            .is_err()
    );
    assert_eq!(active.state(), SecretLeaseState::Expired);

    let mut revoked = lease(2, reference, delivery.clone(), 1, u64::MAX);
    revoked.revoke();
    assert!(
        revoked
            .consume(owner(), environment(), digest(3), digest(4), reference, &delivery, 10)
            .is_err()
    );
    assert!(peritus_secrets::secret_delivery_exact(true, true, true));
    assert!(!peritus_secrets::secret_delivery_exact(true, false, true));
}

#[test]
fn every_delivery_mode_is_owned_redacted_and_idempotently_cleaned() {
    let temporary = tempfile::tempdir().unwrap();
    let canary = b"DELIVERY-CANARY";
    let reference = reference(canary);
    let deliveries = [
        SecretDelivery::Environment(EnvironmentName::new("TOKEN").unwrap()),
        SecretDelivery::File(SandboxPath::new("/run/peritus/token").unwrap()),
        SecretDelivery::BrokeredHandle(BrokeredHandleLabel::new("token-handle").unwrap()),
    ];
    let mut session = SecretDeliverySession::new();
    for (index, delivery) in deliveries.into_iter().enumerate() {
        let lease = lease(u8::try_from(index + 1).unwrap(), reference, delivery, 1, u64::MAX);
        session
            .deliver(
                lease,
                SecretMaterial::new(canary.to_vec()).unwrap(),
                SecretDeliveryContext::new(owner(), environment(), digest(3), digest(4), 10),
                temporary.path(),
            )
            .unwrap();
        assert_eq!(session.leases().last().unwrap().state(), SecretLeaseState::Exhausted);
    }
    assert_eq!(session.artifacts().len(), 3);
    assert!(!format!("{session:?}").contains("DELIVERY-CANARY"));
    let staged: Vec<_> = session
        .artifacts()
        .iter()
        .filter_map(|artifact| artifact.file_paths().map(|(path, _)| path.to_owned()))
        .collect();
    assert_eq!(staged.len(), 1);
    assert!(staged[0].is_file());
    session.release().unwrap();
    session.release().unwrap();
    assert!(!staged[0].exists());
    assert!(session.receipts().iter().all(peritus_secrets::DeliveryReceipt::released));
}

#[test]
fn keyed_redaction_matches_exact_and_fragments_then_expires() {
    let material = SecretMaterial::new(b"prefix-CANARY-value-suffix".to_vec()).unwrap();
    let set = RedactionSet::new(&material, [7; 32], 500).unwrap();
    assert!(set.matches(b"prefix-CANARY-value-suffix", 499));
    assert!(set.matches(b"CANARY", 499));
    assert!(!set.matches(b"ordinary", 499));
    assert!(!set.matches(b"CANARY", 500));
    let debug = format!("{set:?}");
    assert!(!debug.contains("CANARY"));
    assert!(debug.contains("REDACTED"));
}

#[test]
fn preparation_resolves_only_exact_checked_leases_and_owns_cleanup() {
    let temporary = tempfile::tempdir().unwrap();
    let canary = b"PREPARATION-CANARY";
    let reference = reference(canary);
    let delivery = SecretDelivery::Environment(EnvironmentName::new("TOKEN").unwrap());
    let requirement = SecretGrant::new(reference, delivery.clone());
    let store = Arc::new(MemoryCredentialStore::new());
    store.insert(reference, canary.to_vec());
    let preparation = SecretPreparation::new(
        store.clone(),
        vec![lease(11, reference, delivery.clone(), 1, u64::MAX)],
        10,
        temporary.path().to_path_buf(),
    )
    .unwrap();
    let mut session = preparation
        .prepare(owner(), environment(), digest(3), digest(4), std::slice::from_ref(&requirement))
        .unwrap();
    assert_eq!(session.artifacts().len(), 1);
    assert!(!format!("{session:?}").contains("PREPARATION-CANARY"));
    session.release().unwrap();

    let drifted = SecretPreparation::new(
        store,
        vec![lease(12, reference, delivery, 1, u64::MAX)],
        10,
        temporary.path().to_path_buf(),
    )
    .unwrap();
    assert_eq!(
        drifted
            .prepare(
                owner(),
                environment(),
                digest(3),
                digest(44),
                std::slice::from_ref(&requirement),
            )
            .unwrap_err()
            .kind(),
        SecretErrorKind::Revoked,
    );
}

#[test]
fn recovery_touches_only_exact_owned_identity() {
    let reference = reference(b"recovery");
    let record = SecretRecoveryRecord::new(
        owner(),
        SecretLeaseId::new([4; 16]),
        reference,
        digest(4),
        true,
        false,
    );
    assert_eq!(
        record.classify(owner(), Some(SecretLeaseId::new([4; 16])), Some(true)).unwrap(),
        SecretRecoveryState::LiveOwned,
    );
    assert_eq!(
        record
            .classify(
                ProcessId::new([55; 16]).unwrap(),
                Some(SecretLeaseId::new([4; 16])),
                Some(true)
            )
            .unwrap(),
        SecretRecoveryState::Mismatched,
    );
    assert_eq!(record.classify(owner(), None, None).unwrap(), SecretRecoveryState::Indeterminate);
}

fn lease(
    seed: u8,
    reference: SecretReference,
    delivery: SecretDelivery,
    uses: u32,
    expiry: u64,
) -> SecretLease {
    SecretLease::new(
        SecretLeaseId::new([seed; 16]),
        owner(),
        environment(),
        digest(3),
        digest(4),
        reference,
        delivery,
        uses,
        expiry,
    )
    .unwrap()
}

fn owner() -> ProcessId {
    ProcessId::new([1; 16]).unwrap()
}
fn environment() -> EnvironmentId {
    EnvironmentId::new([2; 16]).unwrap()
}
const fn digest(seed: u8) -> Sha256Digest {
    Sha256Digest::new([seed; 32])
}
fn reference(value: &[u8]) -> SecretReference {
    SecretReference::new(ResourceId::new([9; 16]).unwrap(), peritus_codec::sha256(value))
}

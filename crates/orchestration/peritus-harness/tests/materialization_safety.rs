//! Deterministic C1 materialization and confinement tests.

#![allow(clippy::unwrap_used, reason = "fixed checked E1 test corpus")]

mod fixtures_support;

use peritus_artifact_store::{
    ArtifactDigest, ArtifactStore, EncryptionMetadata, MediaType, StoreConfig, WriteRequest,
};
use peritus_harness::domain::RevisionDigest;
use peritus_harness::{
    AuthorizationActions, MaterializationErrorKind, MaterializationPlan, MaterializationReason,
    ObservedFile, ObservedTarget, PlannedFileOperation, WorkspaceSnapshot,
    materialization_authorization_payloads,
};
use peritus_patch::{Preimage, WorkspacePath};
use peritus_types::{ActionId, ResourceId, Sha256Digest, SnapshotId};

#[test]
fn rollback_reason_is_explicit_bounded_and_control_free() {
    let digest = RevisionDigest::new(Sha256Digest::new([7; 32]));
    assert!(MaterializationReason::rollback(digest, "operator-selected ancestor").is_ok());
    assert_eq!(
        MaterializationReason::rollback(digest, "bad\nreason").unwrap_err().kind(),
        MaterializationErrorKind::InvalidPlan,
    );
}

#[test]
fn absent_files_and_malformed_plan_bytes_cannot_be_activated() {
    let path = WorkspacePath::new("runtime/base.txt".to_owned()).unwrap();
    assert_eq!(
        ObservedFile::new(path, Preimage::Absent).unwrap_err().kind(),
        MaterializationErrorKind::InvalidPlan,
    );
    assert_eq!(
        MaterializationPlan::decode_canonical(b"not a canonical plan").unwrap_err().kind(),
        MaterializationErrorKind::Codec,
    );
}

#[test]
fn deterministic_plan_round_trips_exact_artifact_and_snapshot_bindings() {
    let (revision, content) = fixtures_support::genesis_fixture();
    let target = ObservedTarget::new(
        WorkspaceSnapshot::from_c1(&fixtures_support::workspace_snapshot()),
        Vec::new(),
    )
    .unwrap();
    let plan = MaterializationPlan::build(
        fixtures_support::command_id(72),
        fixtures_support::event_id(73),
        &revision,
        target,
        MaterializationReason::Forward,
        None,
    )
    .unwrap();
    assert_eq!(plan.total_bytes(), content.len() as u64);
    assert!(matches!(
        &plan.operations()[0],
        PlannedFileOperation::Install { artifact_digest, byte_length, .. }
            if *artifact_digest == peritus_codec::sha256(&content)
                && *byte_length == content.len() as u64
    ));
    assert_eq!(
        MaterializationPlan::decode_canonical(&plan.canonical_bytes().unwrap()).unwrap(),
        plan,
    );

    let directory = tempfile::tempdir().unwrap();
    let artifacts = ArtifactStore::open(
        StoreConfig::new(directory.path().join("artifacts"), 1_024, 8_192).unwrap(),
    )
    .unwrap();
    let digest = ArtifactDigest::from_sha256(peritus_codec::sha256(&content));
    let mut writer = artifacts
        .begin_write(WriteRequest::new(
            digest,
            content.len() as u64,
            content.len() as u64,
            MediaType::new("text/plain").unwrap(),
            EncryptionMetadata::unencrypted(),
            fixtures_support::event_id(74),
        ))
        .unwrap();
    writer.write_chunk(&content).unwrap();
    writer.finalize().unwrap();
    let actions = AuthorizationActions::new(
        ActionId::new(fixtures_support::bytes(75)).unwrap(),
        ActionId::new(fixtures_support::bytes(76)).unwrap(),
    );
    let first = materialization_authorization_payloads(
        &plan,
        &artifacts,
        ResourceId::new(fixtures_support::bytes(77)).unwrap(),
        actions,
        SnapshotId::new(fixtures_support::bytes(78)).unwrap(),
    )
    .unwrap();
    let second = materialization_authorization_payloads(
        &plan,
        &artifacts,
        ResourceId::new(fixtures_support::bytes(77)).unwrap(),
        actions,
        SnapshotId::new(fixtures_support::bytes(78)).unwrap(),
    )
    .unwrap();
    assert_eq!(first, second);
    assert!(first.patch().starts_with(b"PERITUS-WORKSPACE-PATCH-V1\0"));
    assert!(first.candidate().starts_with(b"PERITUS-WORKSPACE-CANDIDATE-V1\0"));
}

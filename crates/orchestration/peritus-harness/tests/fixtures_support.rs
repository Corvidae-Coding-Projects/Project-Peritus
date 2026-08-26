//! Shared deterministic E1 compatibility-fixture constructors.

#![allow(clippy::unwrap_used, reason = "fixed checked E1 test corpus")]
#![allow(dead_code, reason = "each integration test imports the fixture subset it exercises")]

use peritus_git::{
    CandidateRequest, CreateWorktree, GitRepository, RepositoryOptions, SnapshotRequest,
    WorktreeAccess, WorktreeName,
};
use peritus_harness::domain::{
    AuthoritySet, CheckedHarnessGraph, CompatibilityContract, ComponentContents,
    ComponentDeclaration, ComponentId, ComponentIdentity, ComponentIntegrity, ComponentKind,
    ComponentLocation, ComponentOwnership, ComponentRequirements, GraphEnvironment, HarnessLimits,
    HarnessRevision, LineageSeed, ManifestDigest, MediaType, Owner, Provenance, SchemaInterval,
    SchemaVersion, SourcePath, TargetPath, VerifiedComponentContent,
};
use peritus_test_support::{FixturePath, TemporaryRepositoryBuilder};
use peritus_types::{
    CommandId, EventId, Generation, RevisionNumber, Sha256Digest, SnapshotId, WorkspaceId,
};
use peritus_workspace::SnapshotIdentity;

/// Returns a stable nonzero 16-byte identity fixture.
#[must_use]
pub const fn bytes(value: u8) -> [u8; 16] {
    [value; 16]
}
/// Returns a stable checked command identity fixture.
///
/// # Panics
///
/// Panics when `value` is zero, which is outside the fixed fixture domain.
#[must_use]
pub fn command_id(value: u8) -> CommandId {
    CommandId::new(bytes(value)).unwrap()
}
/// Returns a stable checked event identity fixture.
///
/// # Panics
///
/// Panics when `value` is zero, which is outside the fixed fixture domain.
#[must_use]
pub fn event_id(value: u8) -> EventId {
    EventId::new(bytes(value)).unwrap()
}
/// Returns a stable synthetic SHA-256 value fixture.
#[must_use]
pub const fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}

/// Creates a real Git-backed C1 snapshot fixture.
///
/// # Panics
///
/// Panics when temporary storage or the local Git fixture cannot be created.
#[must_use]
pub fn workspace_snapshot() -> SnapshotIdentity {
    let temp = tempfile::tempdir().unwrap();
    let mut source =
        TemporaryRepositoryBuilder::new(temp.path().join("peritus-test-harness-source"))
            .build()
            .unwrap();
    source.write_text(&FixturePath::new("README.md").unwrap(), "baseline\n").unwrap();
    source.commit_all("baseline").unwrap();
    let repository = GitRepository::open(RepositoryOptions::new(source.root())).unwrap();
    let baseline = repository.resolve_baseline("HEAD").unwrap();
    let worktree = repository
        .create_worktree(CreateWorktree::new(
            WorktreeName::new("harness_writer").unwrap(),
            temp.path().join("harness_writer"),
            baseline,
            WorktreeAccess::Writable,
        ))
        .unwrap();
    let candidate =
        repository.create_candidate(CandidateRequest::new(&worktree, baseline.commit())).unwrap();
    let workspace_id = WorkspaceId::new(bytes(70)).unwrap();
    let snapshot = repository
        .create_snapshot(SnapshotRequest::new(
            &worktree,
            &candidate,
            workspace_id,
            SnapshotId::new(bytes(71)).unwrap(),
            baseline.commit(),
        ))
        .unwrap();
    SnapshotIdentity::new(
        workspace_id,
        Generation::first(),
        RevisionNumber::first(),
        snapshot.commit(),
        snapshot.tree(),
    )
}

/// Creates a complete one-component content-bound genesis revision fixture.
///
/// # Panics
///
/// Panics if any fixed test value fails its checked constructor.
#[must_use]
pub fn genesis_fixture() -> (HarnessRevision, Vec<u8>) {
    let content = b"exact harness component\n".to_vec();
    let version = SchemaVersion::new(1).unwrap();
    let interval = SchemaInterval::new(version, version).unwrap();
    let declaration = ComponentDeclaration::new(
        ComponentIdentity::new(
            ComponentId::new("base.instructions").unwrap(),
            ComponentKind::BaseInstructionFragment,
            version,
        ),
        ComponentLocation::new(
            SourcePath::new(".peritus-harness/components/base.txt").unwrap(),
            TargetPath::new("runtime/base.txt").unwrap(),
            MediaType::new("text/plain").unwrap(),
        ),
        ComponentIntegrity::new(content.len() as u64, peritus_codec::sha256(&content), None),
        ComponentOwnership::new(
            Owner::new("test-owner").unwrap(),
            Provenance::new("fixed integration fixture").unwrap(),
        ),
        ComponentRequirements::new(
            Vec::new(),
            CompatibilityContract::new(interval, Vec::new(), Vec::new()).unwrap(),
            AuthoritySet::empty(),
            ComponentKind::BaseInstructionFragment.protection_class(),
        ),
        HarnessLimits::compiled(),
    )
    .unwrap();
    let graph = CheckedHarnessGraph::check(
        vec![declaration.clone()],
        &GraphEnvironment::new(Vec::new(), Vec::new()).unwrap(),
        HarnessLimits::compiled(),
    )
    .unwrap();
    let verified = VerifiedComponentContent::new(&declaration, content.clone()).unwrap();
    let contents = ComponentContents::new(&graph, vec![verified]).unwrap();
    let revision = HarnessRevision::genesis(
        LineageSeed::new(digest(90)),
        ManifestDigest::new(digest(91)),
        graph,
        &contents,
    )
    .unwrap();
    (revision, content)
}

#[test]
fn fixed_fixture_constructs_a_content_bound_genesis() {
    let (revision, content) = genesis_fixture();
    assert_eq!(revision.number().get(), 1);
    assert_eq!(revision.artifact_roots()[0].content_digest(), peritus_codec::sha256(&content));
}

//! Real Git-backed C1 manifest inventory and artifact-finalization tests.

#![allow(clippy::unwrap_used, reason = "fixed checked E1 test corpus")]

use core::fmt::Write as _;

use peritus_artifact_store::{ArtifactStore, StoreConfig};
use peritus_git::{
    CandidateRequest, CreateWorktree, GitRepository, RepositoryOptions, SnapshotRequest,
    WorktreeAccess, WorktreeName,
};
use peritus_harness::domain::HarnessLimits;
use peritus_harness::{ManifestErrorKind, load_harness};
use peritus_test_support::{FixturePath, TemporaryRepository, TemporaryRepositoryBuilder};
use peritus_types::{Generation, RevisionNumber, SnapshotId, WorkspaceId};
use peritus_workspace::{ReadOnlyOpenRequest, ReadOnlyWorkspace, SnapshotIdentity};
use tempfile::TempDir;

const CONTENT: &[u8] = b"exact harness component\n";

#[derive(Clone, Copy)]
enum Corpus {
    Valid,
    Missing,
    Undeclared,
    Drifted,
}

struct HarnessWorkspace {
    temp: TempDir,
    _source: TemporaryRepository,
    workspace: ReadOnlyWorkspace,
}

impl HarnessWorkspace {
    fn open(corpus: Corpus) -> Self {
        let temp = tempfile::tempdir().unwrap();
        let mut source =
            TemporaryRepositoryBuilder::new(temp.path().join("peritus-test-harness-loader-source"))
                .build()
                .unwrap();
        source
            .write(&FixturePath::new(".peritus-harness/manifest.toml").unwrap(), &manifest())
            .unwrap();
        if matches!(corpus, Corpus::Missing) {
            source
                .write(
                    &FixturePath::new(".peritus-harness/components/observed.txt").unwrap(),
                    b"observed but undeclared\n",
                )
                .unwrap();
        } else {
            let mut content = CONTENT.to_vec();
            if matches!(corpus, Corpus::Drifted) {
                content[0] = b'X';
            }
            source
                .write(&FixturePath::new(".peritus-harness/components/base.txt").unwrap(), &content)
                .unwrap();
        }
        if matches!(corpus, Corpus::Undeclared) {
            source
                .write(
                    &FixturePath::new(".peritus-harness/components/extra.txt").unwrap(),
                    b"undeclared\n",
                )
                .unwrap();
        }
        source.commit_all("harness fixture").unwrap();

        let repository = GitRepository::open(RepositoryOptions::new(source.root())).unwrap();
        let baseline = repository.resolve_baseline("HEAD").unwrap();
        let writer = repository
            .create_worktree(CreateWorktree::new(
                WorktreeName::new("harness_loader_writer").unwrap(),
                temp.path().join("harness_loader_writer"),
                baseline,
                WorktreeAccess::Writable,
            ))
            .unwrap();
        let candidate =
            repository.create_candidate(CandidateRequest::new(&writer, baseline.commit())).unwrap();
        let workspace_id = WorkspaceId::new([70; 16]).unwrap();
        let snapshot = repository
            .create_snapshot(SnapshotRequest::new(
                &writer,
                &candidate,
                workspace_id,
                SnapshotId::new([71; 16]).unwrap(),
                baseline.commit(),
            ))
            .unwrap();
        let reader = repository
            .create_worktree(CreateWorktree::new(
                WorktreeName::new("harness_loader_reader").unwrap(),
                temp.path().join("harness_loader_reader"),
                snapshot.baseline(),
                WorktreeAccess::ReadOnly,
            ))
            .unwrap();
        let identity = SnapshotIdentity::new(
            workspace_id,
            Generation::first(),
            RevisionNumber::first(),
            snapshot.commit(),
            snapshot.tree(),
        );
        let workspace = ReadOnlyWorkspace::open(ReadOnlyOpenRequest::new(
            repository,
            reader,
            identity,
            writer.root(),
        ))
        .unwrap();
        Self { temp, _source: source, workspace }
    }
}

#[test]
fn real_c1_snapshot_loads_checks_finalizes_and_constructs_genesis() {
    let fixture = HarnessWorkspace::open(Corpus::Valid);
    let loaded = load_harness(&fixture.workspace, HarnessLimits::compiled()).unwrap();
    assert_eq!(loaded.component_count(), 1);
    let checked = loaded.check().unwrap();
    let artifacts = ArtifactStore::open(
        StoreConfig::new(fixture.temp.path().join("artifacts"), 1_024, 8_192).unwrap(),
    )
    .unwrap();
    let finalized = checked
        .finalize_artifacts(&artifacts, peritus_types::EventId::new([72; 16]).unwrap())
        .unwrap();
    assert_eq!(finalized.len(), 1);
    artifacts.verify(finalized[0].digest()).unwrap();
    let genesis = checked.genesis().unwrap();
    assert_eq!(genesis.graph().declarations().len(), 1);
}

#[test]
fn real_c1_inventory_rejects_missing_undeclared_and_drifted_content() {
    for (corpus, expected) in [
        (Corpus::Missing, ManifestErrorKind::MissingEntry),
        (Corpus::Undeclared, ManifestErrorKind::UndeclaredEntry),
        (Corpus::Drifted, ManifestErrorKind::DigestMismatch),
    ] {
        let fixture = HarnessWorkspace::open(corpus);
        assert_eq!(
            load_harness(&fixture.workspace, HarnessLimits::compiled()).unwrap_err().kind(),
            expected,
        );
    }
}

fn manifest() -> Vec<u8> {
    let mut digest = String::with_capacity(64);
    for byte in peritus_codec::sha256(CONTENT).as_bytes() {
        write!(digest, "{byte:02x}").expect("writing to a String cannot fail");
    }
    format!(
        r#"schema_version = 1
lineage_seed = "{lineage}"
provider_features = []
platform_features = []

[limits]
components = 2

[[components]]
id = "base.instructions"
kind = "base_instruction_fragment"
schema_version = 1
source_path = ".peritus-harness/components/base.txt"
target_path = "runtime/base.txt"
media_type = "text/plain"
byte_length = 24
content_sha256 = "{digest}"
owner = "test-owner"
provenance = "real C1 loader fixture"
dependencies = []
declared_authority = []
protection_class = "evolvable"

[components.compatibility]
minimum_schema = 1
maximum_schema = 1
provider_features = []
platform_features = []
"#,
        lineage = "11".repeat(32),
    )
    .into_bytes()
}

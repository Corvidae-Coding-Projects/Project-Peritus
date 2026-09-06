//! C0 ownership, immutable artifacts, transactional roots, and restart fixtures.

use super::*;
use peritus_context::{ContextNodeId, working::WorkingBinding};
use peritus_role::HarnessRole;
use peritus_types::{RunId, WorkspaceId};

fn binding() -> WorkingBinding {
    WorkingBinding::new(
        RunId::new([1; 16]).unwrap(),
        WorkspaceId::new([2; 16]).unwrap(),
        ContextNodeId::new([3; 16]).unwrap(),
        HarnessRole::Writer,
        0,
    )
}

#[test]
fn checkpoint_roots_and_exact_artifacts_survive_restart() {
    let root = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let mut store = LocalStore::open(root.path(), workspace.path(), binding()).unwrap();
    let artifact = store.store(b"exact observation beyond any preview").unwrap();
    store.append(b"observation", &[artifact.digest], None).unwrap();
    store.append(b"checkpoint", &[artifact.digest], Some((0, b"manifest-v1".to_vec()))).unwrap();
    assert!(
        store
            .artifacts
            .reference_roots()
            .unwrap()
            .journal()
            .contains(&ArtifactDigest::from_sha256(artifact.digest))
    );
    assert_eq!(store.generation(), 1);
    drop(store);
    let reopened = LocalStore::open(root.path(), workspace.path(), binding()).unwrap();
    assert_eq!(reopened.read(artifact).unwrap(), b"exact observation beyond any preview");
    assert_eq!(reopened.records().unwrap(), vec![b"observation".to_vec(), b"checkpoint".to_vec()]);
    assert_eq!(reopened.checkpoint_root().unwrap(), Some(b"manifest-v1".to_vec()));
    assert_eq!(reopened.generation(), 1);
}

#[test]
fn unpublished_candidate_and_stale_generation_preserve_previous_checkpoint() {
    let root = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let mut store = LocalStore::open(root.path(), workspace.path(), binding()).unwrap();
    store.append(b"checkpoint", &[], Some((0, b"old".to_vec()))).unwrap();
    let uncommitted = store.store(b"candidate-not-published").unwrap();
    assert!(store.append(b"stale", &[uncommitted.digest], Some((0, b"bad".to_vec()))).is_err());
    drop(store);
    let reopened = LocalStore::open(root.path(), workspace.path(), binding()).unwrap();
    assert_eq!(reopened.checkpoint_root().unwrap(), Some(b"old".to_vec()));
    assert_eq!(reopened.sequence(), 1);
}

#[test]
fn a_missing_artifact_aborts_the_entire_c0_transaction() {
    let root = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let mut store = LocalStore::open(root.path(), workspace.path(), binding()).unwrap();
    assert!(
        store
            .append(b"checkpoint", &[sha256(b"not finalized")], Some((0, b"bad".to_vec())))
            .is_err()
    );
    assert_eq!(store.sequence(), 0);
    assert!(store.checkpoint_root().unwrap().is_none());
}

#[test]
fn ownership_and_scope_fail_closed() {
    let root = tempfile::tempdir().unwrap();
    let workspace = tempfile::tempdir().unwrap();
    let owner = LocalStore::open(root.path(), workspace.path(), binding()).unwrap();
    assert!(LocalStore::open(root.path(), workspace.path(), binding()).is_err());
    drop(owner);
    let b = binding();
    let reviewer = WorkingBinding::new(b.run(), b.workspace(), b.task(), HarnessRole::Reviewer, 0);
    assert!(LocalStore::open(root.path(), workspace.path(), reviewer).is_err());
    assert!(LocalStore::open(&workspace.path().join("memory"), workspace.path(), b).is_err());
    assert!(!workspace.path().join("memory").exists());
}

//! Persistent mixed-state recovery, exact authority, and preservation of human work.

use super::super::{git, tests::repository};
use super::*;

#[path = "tests/crash.rs"]
mod crash;

struct Fixture {
    repository: tempfile::TempDir,
    records: tempfile::TempDir,
    path: PathBuf,
    binding: [u8; 32],
    digest: [u8; 32],
}

impl Fixture {
    fn new() -> Self {
        let repository = repository();
        let root = repository.path();
        fs::write(root.join("tracked.txt"), "prior unstaged\n").unwrap();
        let baseline = ManagedBaseline::capture(root, true).unwrap();
        fs::write(root.join("tracked.txt"), "candidate tracked\n").unwrap();
        fs::write(root.join("unrelated.txt"), "candidate other\n").unwrap();
        git(root, &["add", "tracked.txt", "unrelated.txt"], None).unwrap();
        Self::changed(repository, baseline)
    }

    fn changed(repository: tempfile::TempDir, baseline: ManagedBaseline) -> Self {
        let root = repository.path();
        let paths = baseline.changed_paths(root).unwrap();
        let candidate = WorkspaceCheckpoint::capture(root).unwrap().digest().into_bytes();
        let records = tempfile::tempdir().unwrap();
        let path = records.path().join("discard.state");
        let binding = [3_u8; 32];
        let digest = prepare(root, baseline, paths, &path, binding, candidate).unwrap();
        Self { repository, records, path, binding, digest }
    }

    fn restore_first_source(&self) {
        let mut state = state::read(&self.path).unwrap().unwrap();
        state.phase = Phase::Restoring;
        state::save(&self.path, &state).unwrap();
        fs::write(self.repository.path().join("tracked.txt"), "prior unstaged\n").unwrap();
    }
}

#[test]
fn preparation_and_repeated_inspection_are_inert() {
    let fixture = Fixture::new();
    let root = fixture.repository.path();
    let before = WorkspaceCheckpoint::capture(root).unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    for _ in 0..3 {
        assert_eq!(
            inspect(&fixture.path, fixture.binding, fixture.digest).unwrap(),
            Some(DiscardTransactionState::Prepared)
        );
    }
    assert_eq!(WorkspaceCheckpoint::capture(root).unwrap(), before);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
}

#[test]
fn unchanged_candidate_can_complete_discard_without_filesystem_effects() {
    let repository = repository();
    let root = repository.path();
    let before = WorkspaceCheckpoint::capture(root).unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    let baseline = ManagedBaseline::capture(root, true).unwrap();
    let records = tempfile::tempdir().unwrap();
    let path = records.path().join("discard.state");
    let binding = [3_u8; 32];
    let digest =
        prepare(root, baseline, Vec::new(), &path, binding, before.digest().into_bytes()).unwrap();
    assert_eq!(execute(&path, binding, digest).unwrap(), Vec::<PathBuf>::new());
    assert_eq!(WorkspaceCheckpoint::capture(root).unwrap(), before);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    assert_eq!(
        inspect(&path, binding, digest).unwrap(),
        Some(DiscardTransactionState::Completed(Vec::new()))
    );
}

#[test]
fn explicit_retry_completes_a_persisted_partial_source_restore() {
    let fixture = Fixture::new();
    fixture.restore_first_source();
    assert_eq!(
        inspect(&fixture.path, fixture.binding, fixture.digest).unwrap(),
        Some(DiscardTransactionState::Restoring)
    );
    execute(&fixture.path, fixture.binding, fixture.digest).unwrap();
    let root = fixture.repository.path();
    assert_eq!(fs::read(root.join("tracked.txt")).unwrap(), b"prior unstaged\n");
    assert_eq!(fs::read(root.join("unrelated.txt")).unwrap(), b"committed\n");
    assert_eq!(git(root, &["show", ":tracked.txt"], None).unwrap(), b"committed\n");
    assert_eq!(
        inspect(&fixture.path, fixture.binding, fixture.digest).unwrap(),
        Some(DiscardTransactionState::Completed(Vec::new()))
    );
}

#[test]
fn retry_cleans_only_a_resumable_owned_unsealed_preparation() {
    let fixture = Fixture::new();
    let mut state = state::read(&fixture.path).unwrap().unwrap();
    state.phase = Phase::Restoring;
    let owner = Owner::acquire(&fixture.path).unwrap();
    let directory = state.plan.root.join(".peritus-restore-interrupted");
    fs::create_dir(&directory).unwrap();
    let mut journal = Journal { path: fixture.path.clone(), state, _owner: owner };
    own_directory(&directory, Kind::Source, Some(&mut journal)).unwrap();
    fs::write(directory.join("source"), b"partial preimage").unwrap();
    drop(journal);

    execute(&fixture.path, fixture.binding, fixture.digest).unwrap();
    assert_eq!(
        fs::read(fixture.repository.path().join("tracked.txt")).unwrap(),
        b"prior unstaged\n"
    );
    assert!(!directory.exists());
}

#[test]
fn completed_retry_preserves_later_human_edits() {
    let fixture = Fixture::new();
    execute(&fixture.path, fixture.binding, fixture.digest).unwrap();
    let root = fixture.repository.path();
    fs::write(root.join("tracked.txt"), "later human work\n").unwrap();
    git(root, &["add", "tracked.txt"], None).unwrap();
    let before = WorkspaceCheckpoint::capture(root).unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    execute(&fixture.path, fixture.binding, fixture.digest).unwrap();
    assert_eq!(WorkspaceCheckpoint::capture(root).unwrap(), before);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
}

#[test]
fn conflicting_source_and_index_values_are_preserved_before_retry_effects() {
    for staging in [false, true] {
        let fixture = Fixture::new();
        fixture.restore_first_source();
        let root = fixture.repository.path();
        fs::write(root.join("unrelated.txt"), "third human value\n").unwrap();
        if staging {
            git(root, &["add", "unrelated.txt"], None).unwrap();
        }
        let before = WorkspaceCheckpoint::capture(root).unwrap();
        let index = fs::read(root.join(".git/index")).unwrap();
        assert!(execute(&fixture.path, fixture.binding, fixture.digest).is_err());
        assert_eq!(WorkspaceCheckpoint::capture(root).unwrap(), before);
        assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
    }
}

#[test]
fn foreign_authority_and_corrupt_records_never_restore() {
    let fixture = Fixture::new();
    let root = fixture.repository.path();
    let before = WorkspaceCheckpoint::capture(root).unwrap();
    assert!(execute(&fixture.path, [4_u8; 32], fixture.digest).is_err());
    assert!(execute(&fixture.path, fixture.binding, [9_u8; 32]).is_err());
    let mut bytes = fs::read(&fixture.path).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    fs::write(&fixture.path, bytes).unwrap();
    assert!(inspect(&fixture.path, fixture.binding, fixture.digest).is_err());
    assert!(execute(&fixture.path, fixture.binding, fixture.digest).is_err());
    assert_eq!(WorkspaceCheckpoint::capture(root).unwrap(), before);
}

#[test]
fn foreign_git_locks_reject_before_source_effects_and_remain_intact() {
    let fixture = Fixture::new();
    fixture.restore_first_source();
    let root = fixture.repository.path();
    let lock = root.join(".git/index.lock");
    fs::write(&lock, "another Git operation\n").unwrap();
    let before = WorkspaceCheckpoint::capture(root).unwrap();
    assert!(execute(&fixture.path, fixture.binding, fixture.digest).is_err());
    assert_eq!(fs::read(&lock).unwrap(), b"another Git operation\n");
    assert_eq!(WorkspaceCheckpoint::capture(root).unwrap(), before);
}

#[test]
fn changed_index_only_is_preserved_even_when_source_matches_a_preimage() {
    let fixture = Fixture::new();
    fixture.restore_first_source();
    let root = fixture.repository.path();
    fs::write(root.join("unrelated.txt"), "third human index value\n").unwrap();
    git(root, &["add", "unrelated.txt"], None).unwrap();
    fs::write(root.join("unrelated.txt"), "candidate other\n").unwrap();
    let before = WorkspaceCheckpoint::capture(root).unwrap();
    let index = fs::read(root.join(".git/index")).unwrap();
    assert!(execute(&fixture.path, fixture.binding, fixture.digest).is_err());
    assert_eq!(WorkspaceCheckpoint::capture(root).unwrap(), before);
    assert_eq!(fs::read(root.join(".git/index")).unwrap(), index);
}

#[test]
fn unrelated_later_staging_survives_a_partial_discard_retry() {
    let fixture = Fixture::new();
    fixture.restore_first_source();
    let root = fixture.repository.path();
    fs::write(root.join("human.txt"), "human work outside owned paths\n").unwrap();
    git(root, &["add", "human.txt"], None).unwrap();
    execute(&fixture.path, fixture.binding, fixture.digest).unwrap();
    assert_eq!(fs::read(root.join("human.txt")).unwrap(), b"human work outside owned paths\n");
    assert_eq!(
        git(root, &["show", ":human.txt"], None).unwrap(),
        b"human work outside owned paths\n"
    );
}

#[test]
fn replaced_staging_entries_are_preserved() {
    let fixture = Fixture::new();
    let mut state = state::read(&fixture.path).unwrap().unwrap();
    let owner = Owner::acquire(&fixture.path).unwrap();
    let directory = state.plan.root.join(".peritus-restore-test");
    fs::create_dir(&directory).unwrap();
    state.phase = Phase::Restoring;
    let mut journal = Journal { path: fixture.path.clone(), state, _owner: owner };
    own_directory(&directory, Kind::Source, Some(&mut journal)).unwrap();
    fs::create_dir(directory.join("source")).unwrap();
    fs::write(directory.join("source/human.txt"), "preserve\n").unwrap();
    drop(journal);
    assert!(execute(&fixture.path, fixture.binding, fixture.digest).is_err());
    assert_eq!(fs::read(directory.join("source/human.txt")).unwrap(), b"preserve\n");
}

#[test]
fn competing_recovery_owner_prevents_all_effects() {
    let fixture = Fixture::new();
    let owner = Owner::acquire(&fixture.path).unwrap();
    let before = WorkspaceCheckpoint::capture(fixture.repository.path()).unwrap();
    assert!(execute(&fixture.path, fixture.binding, fixture.digest).is_err());
    assert_eq!(WorkspaceCheckpoint::capture(fixture.repository.path()).unwrap(), before);
    drop(owner);
    execute(&fixture.path, fixture.binding, fixture.digest).unwrap();
}

#[cfg(unix)]
#[test]
fn symlink_records_and_owner_files_preserve_their_targets() {
    for owner in [false, true] {
        let fixture = Fixture::new();
        let target = fixture.records.path().join("human.txt");
        fs::write(&target, "preserve\n").unwrap();
        let path =
            if owner { fixture.path.with_extension("discard-owner") } else { fixture.path.clone() };
        fs::remove_file(&path).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(execute(&fixture.path, fixture.binding, fixture.digest).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"preserve\n");
        assert!(fs::symlink_metadata(&path).unwrap().file_type().is_symlink());
    }
}

#[test]
fn new_repository_is_retained_and_its_removed_root_remains_a_valid_postimage() {
    let root = repository();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    let added = repository();
    let nested = root.path().join("added");
    fs::rename(added.keep(), &nested).unwrap();
    let fixture = Fixture::changed(root, baseline);
    let recovered = execute(&fixture.path, fixture.binding, fixture.digest).unwrap();
    assert_eq!(recovered.len(), 1);
    assert!(!nested.exists());
    assert_eq!(fs::read(recovered[0].join("repository/tracked.txt")).unwrap(), b"committed\n");
    assert_eq!(
        inspect(&fixture.path, fixture.binding, fixture.digest).unwrap(),
        Some(DiscardTransactionState::Completed(recovered))
    );
}

#[test]
fn a_manually_removed_repository_is_not_mistaken_for_an_owned_archive() {
    let root = repository();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    let added = repository();
    let nested = root.path().join("added");
    fs::rename(added.keep(), &nested).unwrap();
    let fixture = Fixture::changed(root, baseline);
    fs::remove_dir_all(nested).unwrap();
    let before = fs::read(fixture.repository.path().join(".git/index")).unwrap();
    assert!(execute(&fixture.path, fixture.binding, fixture.digest).is_err());
    assert_eq!(fs::read(fixture.repository.path().join(".git/index")).unwrap(), before);
}

#[test]
fn replacing_a_plain_directory_with_git_can_be_discarded_back_to_its_files() {
    let root = repository();
    let nested = root.path().join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("prior.txt"), "prior plain directory\n").unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    git(&nested, &["init", "--quiet"], None).unwrap();
    fs::write(nested.join("prior.txt"), "candidate\n").unwrap();
    let fixture = Fixture::changed(root, baseline);
    let recovered = execute(&fixture.path, fixture.binding, fixture.digest).unwrap();
    assert_eq!(recovered.len(), 1);
    assert!(!nested.join(".git").exists());
    assert_eq!(fs::read(nested.join("prior.txt")).unwrap(), b"prior plain directory\n");
}

#[test]
fn deleted_original_repository_is_reconstructed_from_its_independent_backup() {
    let root = repository();
    let child = repository();
    let nested = root.path().join("nested");
    fs::rename(child.keep(), &nested).unwrap();
    fs::write(nested.join("tracked.txt"), "prior nested unstaged\n").unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::remove_dir_all(&nested).unwrap();
    let fixture = Fixture::changed(root, baseline);
    execute(&fixture.path, fixture.binding, fixture.digest).unwrap();
    assert_eq!(fs::read(nested.join("tracked.txt")).unwrap(), b"prior nested unstaged\n");
    assert_eq!(git(&nested, &["show", ":tracked.txt"], None).unwrap(), b"committed\n");
}

#[test]
fn committed_nested_head_restores_without_rewriting_the_previous_branch() {
    let root = repository();
    let child = repository();
    let nested = root.path().join("nested");
    fs::rename(child.keep(), &nested).unwrap();
    let original = git(&nested, &["rev-parse", "HEAD"], None).unwrap();
    let baseline = ManagedBaseline::capture(root.path(), true).unwrap();
    fs::write(nested.join("tracked.txt"), "candidate committed\n").unwrap();
    git(&nested, &["commit", "-am", "candidate"], None).unwrap();
    let candidate = git(&nested, &["rev-parse", "HEAD"], None).unwrap();
    let branch = git(&nested, &["symbolic-ref", "HEAD"], None).unwrap();
    let branch = std::str::from_utf8(&branch).unwrap().trim();
    let fixture = Fixture::changed(root, baseline);
    let recovered = execute(&fixture.path, fixture.binding, fixture.digest).unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(git(&nested, &["rev-parse", "HEAD"], None).unwrap(), original);
    assert_eq!(git(&nested, &["rev-parse", branch], None).unwrap(), candidate);
    assert!(recovered[0].join("head.json").is_file());
}

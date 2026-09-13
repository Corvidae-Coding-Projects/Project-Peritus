use super::{Snapshot, fingerprint_paths};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("peritus-proof-inputs-{}-{id}", std::process::id()));
        fs::create_dir_all(&root).expect("create proof inputs");
        Self(root)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _cleanup_result = fs::remove_dir_all(&self.0);
    }
}

fn snapshot(sources: BTreeMap<String, String>) -> Snapshot {
    Snapshot {
        git_commit: "a".repeat(40),
        git_tree: "b".repeat(40),
        protected_base: Some("c".repeat(40)),
        matches_committed_sources: true,
        source_sha256: sources,
    }
}

#[test]
fn source_changes_during_a_run_invalidate_its_success_summary() {
    let fixture = Fixture::new();
    let input = fixture.0.join("verified.rs");
    fs::write(&input, b"original source\n").expect("initial source");
    let paths = BTreeSet::from([input.clone()]);
    let before = snapshot(fingerprint_paths(&fixture.0, &paths).expect("before hashes"));
    let unchanged = snapshot(fingerprint_paths(&fixture.0, &paths).expect("unchanged hashes"));
    before.require_unchanged(&unchanged).expect("stable input identities");
    fs::write(&input, b"changed source\n").expect("modify source");
    let after = snapshot(fingerprint_paths(&fixture.0, &paths).expect("after hashes"));
    assert!(before.require_unchanged(&after).is_err());
    fs::remove_file(&input).expect("remove source");
    assert!(fingerprint_paths(&fixture.0, &paths).is_err());
}

#[test]
fn added_or_removed_inputs_and_changed_git_identities_invalidate_a_run() {
    let mut before = snapshot(BTreeMap::from([("a.rs".to_owned(), "a".repeat(64))]));
    let mut after = snapshot(BTreeMap::new());
    assert!(before.require_unchanged(&after).is_err());
    assert!(after.require_unchanged(&before).is_err());
    before.source_sha256.clear();
    after.git_commit = "d".repeat(40);
    assert!(before.require_unchanged(&after).is_err());
    after.git_commit.clone_from(&before.git_commit);
    after.protected_base = Some("e".repeat(40));
    assert!(before.require_unchanged(&after).is_err());
}

#[test]
fn ci_requires_the_event_candidate_clean_sources_and_an_explicit_base() {
    let mut value = snapshot(BTreeMap::new());
    let candidate = value.git_commit.clone();
    value.validate_ci_identity(&candidate).expect("exact clean candidate");
    assert!(value.validate_ci_identity(&"d".repeat(40)).is_err());
    value.matches_committed_sources = false;
    assert!(value.validate_ci_identity(&candidate).is_err());
    value.matches_committed_sources = true;
    value.protected_base = None;
    assert!(value.validate_ci_identity(&candidate).is_err());
    value.protected_base = Some("HEAD~1".to_owned());
    assert!(value.validate_ci_identity(&candidate).is_err());
}

#[test]
fn ci_requires_a_comparison_base_distinct_from_the_candidate() {
    let mut value = snapshot(BTreeMap::new());
    let candidate = value.git_commit.clone();
    value.protected_base = Some(candidate.clone());
    assert!(value.validate_ci_identity(&candidate).is_err());
}

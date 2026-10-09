//! Router-permit through target-owned C1 candidate and rollback integration.

#[path = "git_dispatch/authority/mod.rs"]
mod authority_support;
#[path = "git_dispatch/support.rs"]
mod support;

use peritus_tools_git::GitMutationOutcome;
use peritus_types::{RevisionNumber, SnapshotId};
use tempfile::TempDir;

use authority_support::{Ids, artifact_store, authorized_patch, workspace_fixture};

#[test]
fn router_dispatches_candidate_snapshot_and_history_preserving_rollback() {
    let temp = TempDir::new().expect("temporary root");
    let base = Ids::new();
    let mut fixture = workspace_fixture(&temp, &base, "git-dispatch");
    let artifacts = artifact_store(&temp, "git-dispatch-artifacts", 1_048_576);
    let mutation = authorized_patch(&temp, &base, &mut fixture.gateway, fixture.patch);

    let candidate_snapshot = SnapshotId::new([81; 16]).expect("candidate snapshot");
    let candidate_parent = base.for_tool_action(51, "git.candidate");
    let candidate_lower = base.for_action_revision(21, RevisionNumber::first());
    let candidate_json =
        format!(r#"{{"snapshot_id":"{}"}}"#, support::snapshot_hex(candidate_snapshot));
    let (router, prepared) =
        support::prepare(&candidate_parent, "git.candidate", support::arguments(&candidate_json));
    let (outcome, candidate) = support::dispatch_candidate(
        &temp,
        &candidate_lower,
        &candidate_parent,
        &mut fixture.gateway,
        &mutation,
        candidate_snapshot,
        &artifacts,
        prepared,
        router,
    );
    support::assert_success(outcome);
    let Some(GitMutationOutcome::Candidate(candidate)) = candidate else {
        panic!("candidate outcome was not retained");
    };
    assert_eq!(candidate.snapshot().snapshot_id(), candidate_snapshot);
    assert_eq!(fixture.gateway.state().revision(), RevisionNumber::new(2).expect("revision two"));
    assert!(fixture.gateway.state().binding().root().join("authorized.txt").is_file());

    let revision_two = RevisionNumber::new(2).expect("revision two");
    let rollback_base = base.for_action_revision(22, revision_two);
    let rollback_parent = rollback_base.for_tool_action(61, "git.rollback");
    let successor = SnapshotId::new([82; 16]).expect("rollback successor");
    let rollback_json = format!(
        r#"{{"successor_snapshot_id":"{}","target_snapshot_id":"{}"}}"#,
        support::snapshot_hex(successor),
        support::snapshot_hex(fixture.initial.snapshot_id())
    );
    let (router, prepared) =
        support::prepare(&rollback_parent, "git.rollback", support::arguments(&rollback_json));
    let (outcome, rollback) = support::dispatch_rollback(
        &temp,
        &rollback_base,
        &rollback_parent,
        &mut fixture.gateway,
        &fixture.initial,
        successor,
        &artifacts,
        prepared,
        router,
    );
    support::assert_success(outcome);
    let Some(GitMutationOutcome::Rollback(rollback)) = rollback else {
        panic!("rollback outcome was not retained");
    };
    assert_eq!(rollback.snapshot().snapshot_id(), successor);
    assert_eq!(rollback.restored_from(), fixture.initial.commit());
    assert_eq!(fixture.gateway.state().revision(), RevisionNumber::new(3).expect("revision three"));
    assert!(!fixture.gateway.state().binding().root().join("authorized.txt").exists());
    assert_eq!(
        std::fs::read(fixture.gateway.state().binding().root().join("README.md"))
            .expect("restored README"),
        b"baseline\n"
    );
}

use peritus_leases::{LeaseScope, ReconciliationCorrelation};
use peritus_types::{EventId, Generation};
use peritus_workspace::{ErrorCode, RestartDisposition, WorkspaceOperation};
use tempfile::TempDir;

use super::authority_support::{Ids, artifact_store, try_reopen_fixture_at, workspace_fixture};

#[test]
fn open_rejects_a_transaction_root_that_contains_the_managed_worktree() {
    let temp = TempDir::new().expect("temporary root");
    let ids = Ids::new();
    let fixture = workspace_fixture(&temp, &ids, "broad-transaction-root");
    let persistence = fixture.persistence.clone();
    drop(fixture.gateway.into_workspace());
    let error = try_reopen_fixture_at(&persistence, &ids, temp.path())
        .err()
        .expect("broad transaction root must be rejected");
    assert_eq!(error.code(), ErrorCode::InvalidInput);
    assert_eq!(error.operation(), WorkspaceOperation::Open);
}

#[test]
fn reconciliation_observes_unrelated_namespace_child_without_quarantining_it() {
    let temp = TempDir::new().expect("temporary root");
    let ids = Ids::new();
    let fixture = workspace_fixture(&temp, &ids, "unrelated-transaction-child");
    let mut gateway = fixture.gateway;
    let unrelated = gateway.transaction_namespace().join("txn-unrelated");
    std::fs::create_dir(&unrelated).expect("unrelated directory");
    std::fs::write(unrelated.join("sentinel"), b"unchanged\n").expect("sentinel");
    let artifacts = artifact_store(&temp, "unrelated-reconciliation-artifacts", 1_048_576);
    let expected = ReconciliationCorrelation::new(
        LeaseScope::new(ids.workspace, ids.resource, ids.environment),
        Generation::first(),
        ids.holder(),
    );
    let outcome = gateway
        .reconcile_restart(
            expected,
            &artifacts,
            EventId::new([86; 16]).expect("reconciliation event"),
        )
        .expect("reconciliation outcome");
    assert_eq!(outcome.observation().disposition(), RestartDisposition::Dirty);
    assert_eq!(
        std::fs::read(unrelated.join("sentinel")).expect("sentinel remains"),
        b"unchanged\n"
    );
    assert!(unrelated.is_dir());
    assert!(
        !gateway
            .transaction_namespace()
            .read_dir()
            .expect("namespace entries")
            .filter_map(Result::ok)
            .any(|entry| entry
                .file_name()
                .to_string_lossy()
                .starts_with("txn-unrelated.quarantine"))
    );
}

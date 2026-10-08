//! Bound discard authority survives daemon reconciliation without changing live workspace state.

use super::*;
use crate::product_run::deliverable::discard::Pending;
use peritus_product_runner::ProductRunner;
use sha2::{Digest as _, Sha256};

struct Fixture {
    repository: tempfile::TempDir,
    _state: tempfile::TempDir,
    running: ProductRunService,
    run: RunId,
    workspace: WorkspaceId,
    original: Vec<u8>,
}

impl Fixture {
    async fn new() -> Self {
        let repository = repository();
        let original = fs::read(repository.path().join("src/lib.rs")).unwrap();
        let state = tempfile::tempdir().unwrap();
        let writer = scripted(0x21, "writer", complete_writer(CORRECT));
        let reviewer = scripted(0x22, "reviewer", clean_review());
        let fixer = scripted(0x23, "fixer", Vec::new());
        let workspace = WorkspaceId::new([0x24; 16]).unwrap();
        let run = RunId::new([0x25; 16]).unwrap();
        let running =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        running
            .start(
                ProductRunRequest::new(
                    run,
                    workspace,
                    ProductProviderSelection::new(
                        writer.profile.profile_id(),
                        reviewer.profile.profile_id(),
                        fixer.profile.profile_id(),
                    ),
                    "Return 42 with verified tests.".to_owned(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let terminal = wait_for_terminal(&running, run).await;
        assert_eq!(terminal.phase(), ProductRunPhase::Complete, "{}", terminal.summary());
        let status = std::process::Command::new("git")
            .current_dir(repository.path())
            .args(["add", "src/lib.rs"])
            .status()
            .unwrap();
        assert!(status.success());
        Self { repository, _state: state, running, run, workspace, original }
    }

    fn pending(&self) -> Pending {
        let records = self.running.inner.records.read().unwrap();
        Pending::prepare(&self.running.inner.directory, &records[&self.run], self.repository.path())
            .unwrap()
            .unwrap()
    }

    fn partial_source(&self) {
        let path = self
            .running
            .inner
            .directory
            .join(format!("{:032x}.discard-state", u128::from_be_bytes(self.run.into_bytes())));
        let original = fs::read(&path).unwrap();
        let mut state: serde_json::Value = serde_json::from_slice(&original[40..]).unwrap();
        state["phase"] = serde_json::Value::String("Restoring".to_owned());
        let payload = serde_json::to_vec(&state).unwrap();
        let mut bytes = b"P5DS".to_vec();
        bytes.extend(u32::try_from(payload.len()).unwrap().to_be_bytes());
        bytes.extend(Sha256::digest(&payload));
        bytes.extend(payload);
        fs::write(path, bytes).unwrap();
        fs::write(self.repository.path().join("src/lib.rs"), &self.original).unwrap();
    }

    fn reconcile(&self) -> BTreeMap<RunId, super::super::RunRecord> {
        let mut records = self.running.load_test_records().unwrap();
        super::super::recovery::reconcile_restored_candidates(
            &self.running.inner.directory,
            &mut records,
            &BTreeMap::from([(self.workspace, self.repository.path().to_path_buf())]),
        )
        .unwrap();
        records
    }
}

#[test]
fn restart_keeps_original_partial_discard_binding_and_only_exact_discard_can_resume() {
    interaction::block_on(async {
        let fixture = Fixture::new().await;
        let accepted = fixture
            .running
            .control(ProductRunControl::new(fixture.run, ProductRunControlAction::Accept))
            .await
            .unwrap();
        assert!(accepted.deliverable().unwrap().accepted());
        let original_summary = accepted.summary().to_owned();
        let exported = fixture
            .running
            .control(ProductRunControl::new(fixture.run, ProductRunControlAction::Export))
            .await
            .unwrap();
        let export = fs::read(exported.deliverable().unwrap().export_path()).unwrap();
        let original =
            fixture.running.inner.records.read().unwrap()[&fixture.run].checkpoint.unwrap();
        fixture.pending();
        let prepared =
            fixture.running.query_observations(ProductRunQuery::exact(fixture.run)).unwrap();
        let operation = prepared[0].snapshot().operation();
        assert_eq!(operation.kind(), peritus_app_protocol::ProductRunOperationKind::Discard);
        assert_eq!(
            operation.state(),
            peritus_app_protocol::ProductRunOperationState::RecoveryRequired
        );
        assert!(operation.legal_controls().discard());
        assert!(!operation.legal_controls().retry());
        fixture.partial_source();
        let before = ProductRunner::candidate_digest(fixture.repository.path()).unwrap();
        let index = fs::read(fixture.repository.path().join(".git/index")).unwrap();
        let records = fixture.reconcile();
        assert_eq!(records[&fixture.run].checkpoint, Some(original));
        assert!(!records[&fixture.run].candidate_actionable);
        assert!(records[&fixture.run].snapshot.status().contains("retry Discard"));
        assert_eq!(records[&fixture.run].snapshot.summary(), original_summary);
        assert_eq!(ProductRunner::candidate_digest(fixture.repository.path()).unwrap(), before);
        assert_eq!(fs::read(fixture.repository.path().join(".git/index")).unwrap(), index);
        *fixture.running.inner.records.write().unwrap() = records;
        let restoring =
            fixture.running.query_observations(ProductRunQuery::exact(fixture.run)).unwrap();
        assert_eq!(
            restoring[0].snapshot().operation().state(),
            peritus_app_protocol::ProductRunOperationState::OutcomeUnknown
        );
        assert!(restoring[0].snapshot().operation().legal_controls().discard());
        for action in [
            ProductRunControlAction::Accept,
            ProductRunControlAction::Commit,
            ProductRunControlAction::Retry,
        ] {
            assert!(
                fixture.running.control(ProductRunControl::new(fixture.run, action)).await.is_err()
            );
        }
        let saved = fixture
            .running
            .control(ProductRunControl::new(fixture.run, ProductRunControlAction::Export))
            .await
            .unwrap();
        assert_eq!(fs::read(saved.deliverable().unwrap().export_path()).unwrap(), export);
        let request = fixture.running.inner.records.read().unwrap()[&fixture.run].request.clone();
        let new = ProductRunRequest::new(
            RunId::new([0x26; 16]).unwrap(),
            fixture.workspace,
            request.providers(),
            "new work".to_owned(),
        )
        .unwrap();
        assert!(fixture.running.start(new).await.is_err());
        let discarded = fixture
            .running
            .control(ProductRunControl::new(fixture.run, ProductRunControlAction::Discard))
            .await
            .unwrap();
        assert!(discarded.deliverable().unwrap().discarded());
        assert_eq!(discarded.summary(), original_summary);
        {
            let records = fixture.running.inner.records.read().unwrap();
            assert!(records[&fixture.run].interruption_cause.is_empty());
            assert!(records[&fixture.run].remaining_work.is_empty());
        }
        assert_eq!(
            fs::read(fixture.repository.path().join("src/lib.rs")).unwrap(),
            fixture.original
        );
        let restored_index = std::process::Command::new("git")
            .current_dir(fixture.repository.path())
            .args(["show", ":src/lib.rs"])
            .output()
            .unwrap();
        assert!(restored_index.status.success());
        assert_eq!(restored_index.stdout, fixture.original);
        fixture.running.shutdown().await.expect("shutdown product runs");
    });
}

#[test]
fn lost_final_acknowledgement_recovers_from_completed_backend_without_touching_new_edits() {
    interaction::block_on(async {
        let fixture = Fixture::new().await;
        let pending = fixture.pending();
        let original_summary = fixture.running.inner.records.read().unwrap()[&fixture.run]
            .snapshot
            .summary()
            .to_owned();
        fixture.partial_source();
        let records = fixture.reconcile();
        *fixture.running.inner.records.write().unwrap() = records;
        {
            let records = fixture.running.inner.records.read().unwrap();
            pending.execute(&fixture.running.inner.directory, &records[&fixture.run]).unwrap();
        }
        fs::write(fixture.repository.path().join("src/lib.rs"), "later human draft\n").unwrap();
        let index = fs::read(fixture.repository.path().join(".git/index")).unwrap();
        let records = fixture.reconcile();
        assert!(records[&fixture.run].snapshot.deliverable().unwrap().discarded());
        assert_eq!(records[&fixture.run].snapshot.summary(), original_summary);
        assert!(records[&fixture.run].interruption_cause.is_empty());
        assert!(records[&fixture.run].remaining_work.is_empty());
        assert_eq!(
            fs::read(fixture.repository.path().join("src/lib.rs")).unwrap(),
            b"later human draft\n"
        );
        assert_eq!(fs::read(fixture.repository.path().join(".git/index")).unwrap(), index);
        *fixture.running.inner.records.write().unwrap() = records;
        let repeated = fixture
            .running
            .control(ProductRunControl::new(fixture.run, ProductRunControlAction::Discard))
            .await
            .unwrap();
        assert!(repeated.deliverable().unwrap().discarded());
        assert_eq!(
            fs::read(fixture.repository.path().join("src/lib.rs")).unwrap(),
            b"later human draft\n"
        );
        fixture.running.shutdown().await.expect("shutdown product runs");
    });
}

#[test]
fn incomplete_backend_and_foreign_receipt_never_acknowledge_or_change_candidate() {
    interaction::block_on(async {
        let fixture = Fixture::new().await;
        fixture.pending();
        let before = ProductRunner::candidate_digest(fixture.repository.path()).unwrap();
        let records = fixture.reconcile();
        assert!(!records[&fixture.run].snapshot.deliverable().unwrap().discarded());
        assert_eq!(ProductRunner::candidate_digest(fixture.repository.path()).unwrap(), before);
        let path = fixture
            .running
            .inner
            .directory
            .join(format!("{:032x}.discard-intent", u128::from_be_bytes(fixture.run.into_bytes())));
        let mut receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        receipt["plan"] = serde_json::to_value([0_u8; 32]).unwrap();
        fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        let records = fixture.reconcile();
        assert!(!records[&fixture.run].candidate_actionable);
        assert!(!records[&fixture.run].snapshot.deliverable().unwrap().discarded());
        assert!(
            fixture
                .running
                .control(ProductRunControl::new(fixture.run, ProductRunControlAction::Discard))
                .await
                .is_err()
        );
        assert_eq!(ProductRunner::candidate_digest(fixture.repository.path()).unwrap(), before);
        fixture.running.shutdown().await.expect("shutdown product runs");
    });
}

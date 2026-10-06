//! One mutation and recovery frontier across many physical manifest pages.

use super::super::{manifest::TransactionPhase, storage::persist_manifest};
use super::*;

fn many_plan() -> PatchPlan {
    let file = FinalFile::new(b"created".to_vec(), FileMode::Regular, LineEndingPolicy::Preserve)
        .expect("file");
    let mut operations: Vec<_> = (0..1_025)
        .map(|index| {
            PatchOperation::create(
                WorkspacePath::new(format!("a-{index:04}")).expect("path"),
                file.clone(),
            )
        })
        .collect();
    operations.push(
        PatchOperation::replace(
            WorkspacePath::new("z-large").expect("path"),
            Preimage::from_bytes(&vec![19; 8 * 1024 * 1024 + 1], FileMode::Regular),
            file,
        )
        .expect("replace"),
    );
    PatchSet::new(
        binding().workspace_id(),
        binding().generation(),
        binding().revision(),
        operations,
    )
    .expect("one large patch")
    .plan(binding().workspace_id(), binding().generation(), binding().revision())
    .expect("bound")
}

#[test]
fn paged_patch_reopens_rolls_back_and_retries_with_the_same_authority() {
    let workspace = tempfile::tempdir().expect("workspace");
    let transactions = tempfile::tempdir().expect("transactions");
    let before = vec![19; 8 * 1024 * 1024 + 1];
    std::fs::write(workspace.path().join("z-large"), &before).expect("preimage");
    let plan = many_plan();
    let identity = plan.identity();
    let transaction = transactions.path().join(format!("txn-{identity}"));
    let error = apply_with_faults(
        workspace.path(),
        transactions.path(),
        &plan,
        &FailAt::new(vec![
            TransactionFaultPoint::AfterBackupOriginal,
            TransactionFaultPoint::BeforeRollback,
        ]),
    )
    .expect_err("interrupted after all creates and the large backup");
    assert_eq!(error.rollback_status(), RollbackStatus::Indeterminate);
    drop(plan);
    let recovered = recover_transaction(workspace.path(), &transaction, binding()).expect("reopen");
    assert_eq!(recovered.state(), RecoveryState::RolledBackCleanly);
    assert_eq!(recovered.identity(), Some(identity));
    assert_eq!(std::fs::read(workspace.path().join("z-large")).expect("restored"), before);
    assert_eq!(std::fs::read_dir(workspace.path()).expect("entries").count(), 1);

    let plan = many_plan();
    assert_eq!(plan.identity(), identity);
    let applied = apply_with_faults(
        workspace.path(),
        transactions.path(),
        &plan,
        &FailAt::new(vec![TransactionFaultPoint::BeforeCleanup]),
    )
    .expect("same patch progresses after reconciliation");
    assert_eq!(applied.identity(), identity);
    assert_eq!(
        std::fs::read(transaction.join("manifest.bin")).expect("durable receipt"),
        applied.installed_manifest()
    );
    let manifest = Manifest::decode(applied.installed_manifest()).expect("complete paged receipt");
    assert_eq!(manifest.entries.len(), 1_026);
    assert_eq!(manifest.phase, TransactionPhase::Installed);
    drop(plan);
    let recovered =
        recover_transaction(workspace.path(), &transaction, binding()).expect("reopen installed");
    assert_eq!(recovered.state(), RecoveryState::AlreadyApplied);
    assert_eq!(recovered.identity(), Some(identity));
    assert_eq!(std::fs::read_dir(workspace.path()).expect("entries").count(), 1_026);
    assert_eq!(std::fs::read(workspace.path().join("z-large")).expect("final"), b"created");
}

#[test]
fn incomplete_next_manifest_never_replaces_the_predecessor_recovery_root() {
    let transaction = tempfile::tempdir().expect("transaction");
    let plan = many_plan();
    let mut manifest = Manifest::from_plan(&plan, Vec::new());
    persist_manifest(transaction.path(), &manifest, &NoFaults).expect("prepared root");
    let prepared = std::fs::read(transaction.path().join("manifest.bin")).expect("prepared");
    manifest.phase = TransactionPhase::Installing;
    persist_manifest(
        transaction.path(),
        &manifest,
        &FailAt::new(vec![TransactionFaultPoint::BeforePublishManifest]),
    )
    .expect_err("publication interrupted");
    let next = std::fs::OpenOptions::new()
        .write(true)
        .open(transaction.path().join("manifest.next"))
        .expect("unadopted replacement");
    next.set_len(100).expect("simulate incomplete physical page write");
    next.sync_all().expect("sync crash tail");
    assert!(
        Manifest::decode(
            &std::fs::read(transaction.path().join("manifest.next")).expect("partial")
        )
        .is_err()
    );
    let retained = std::fs::read(transaction.path().join("manifest.bin")).expect("predecessor");
    assert_eq!(retained, prepared);
    assert_eq!(
        Manifest::decode(&retained).expect("still readable").phase,
        TransactionPhase::Prepared
    );
    persist_manifest(transaction.path(), &manifest, &NoFaults).expect("replacement can be retried");
    assert_eq!(
        Manifest::decode(&std::fs::read(transaction.path().join("manifest.bin")).expect("adopted"))
            .expect("whole manifest")
            .phase,
        TransactionPhase::Installing
    );
}

#[test]
fn paged_stream_staging_rejects_drift_and_retries_storage_pressure_before_effect() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    #[derive(Debug)]
    struct Retained {
        bytes: &'static [u8],
        fail_once: AtomicBool,
    }
    impl crate::SnapshotSource for Retained {
        fn open(&self) -> io::Result<Box<dyn io::Read + Send>> {
            if self.fail_once.swap(false, Ordering::SeqCst) {
                return Err(io::Error::from(io::ErrorKind::StorageFull));
            }
            Ok(Box::new(io::Cursor::new(self.bytes)))
        }
    }
    for pressure in [false, true] {
        let workspace = tempfile::tempdir().unwrap();
        let transactions = tempfile::tempdir().unwrap();
        let source = crate::SnapshotFile::from_source(
            Arc::new(Retained {
                bytes: if pressure { b"saved" } else { b"drift" },
                fail_once: AtomicBool::new(pressure),
            }),
            peritus_codec::sha256(b"saved"),
            5,
            FileMode::Regular,
        );
        let patch = PatchSet::new(
            binding().workspace_id(),
            binding().generation(),
            binding().revision(),
            vec![
                PatchOperation::create(
                    WorkspacePath::new("a-inline").unwrap(),
                    FinalFile::new(
                        b"inline".to_vec(),
                        FileMode::Regular,
                        LineEndingPolicy::Preserve,
                    )
                    .unwrap(),
                ),
                PatchOperation::create_snapshot(WorkspacePath::new("z-streamed").unwrap(), source),
            ],
        )
        .unwrap();
        let plan = patch
            .plan(binding().workspace_id(), binding().generation(), binding().revision())
            .unwrap();
        let error = apply_with_faults(workspace.path(), transactions.path(), &plan, &NoFaults)
            .expect_err("staging failure");
        assert_eq!(error.rollback_status(), RollbackStatus::NotRequired);
        assert_eq!(std::fs::read_dir(workspace.path()).unwrap().count(), 0);
        assert_eq!(std::fs::read_dir(transactions.path()).unwrap().count(), 0);
        if pressure {
            assert_eq!(error.recovery_class(), crate::RecoveryClass::Retry);
            let applied =
                apply_with_faults(workspace.path(), transactions.path(), &plan, &NoFaults)
                    .expect("same owned source and authority can retry");
            assert_eq!(applied.identity(), plan.identity());
            assert_eq!(std::fs::read(workspace.path().join("z-streamed")).unwrap(), b"saved");
        } else {
            assert_eq!(error.code(), crate::ErrorCode::InvalidContent);
        }
    }
}

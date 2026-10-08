//! Exact resume authority across launch persistence boundaries.

use super::idle::{accept_without_launch, goal, pause_idle, restore, resume_command, start_goal};
use super::*;
use crate::product_run::persistence::{
    PersistenceFaultPoint, inject_persistence_fault, persist_record,
};

#[test]
fn saved_resume_launch_without_worker_is_recovered_by_explicit_replay() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().unwrap();
        let writer = scripted(
            0xa1,
            "saved-resume-writer",
            vec![
                support::text_response(b"First answer."),
                support::text_response(b"Recovered answer."),
            ],
        );
        let reviewer = scripted(0xa2, "saved-resume-reviewer", Vec::new());
        let fixer = scripted(0xa3, "saved-resume-fixer", Vec::new());
        let workspace = WorkspaceId::new([0xa4; 16]).unwrap();
        let run = RunId::new([0xa5; 16]).unwrap();
        let original =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        start_goal(&original, workspace, run, [&writer, &reviewer, &fixer]).await;
        wait_for_terminal(&original, run).await;
        pause_idle(&original, workspace).await;
        let resume = resume_command(&original, workspace);
        accept_without_launch(&original, &resume);
        {
            let mut records = original.inner.records.write().unwrap();
            let record = records.get_mut(&run).unwrap();
            record.attempt_admission = Some(
                peritus_product_runner::control::OperationId::new(resume.operation().into_bytes())
                    .unwrap(),
            );
            record.snapshot =
                crate::product_run::snapshot::initial_snapshot(&record.request).unwrap();
            persist_record(&original.inner.directory, record).unwrap();
        }
        // No worker was spawned after the durable launch projection: simulate process loss.
        drop(original);
        let restored =
            restore(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        assert_eq!(
            restored.inner.records.read().unwrap()[&run].snapshot.phase(),
            ProductRunPhase::RecoveryRequired
        );
        assert_eq!(writer.requests.lock().unwrap().len(), 1, "opening must never spawn work");
        for _ in 0..3 {
            assert!(matches!(
                restored.workbench_receipt(actor(), &resume),
                AppResponsePayload::WorkbenchReceipt(_)
            ));
            assert_eq!(
                restored.inner.records.read().unwrap()[&run].snapshot.phase(),
                ProductRunPhase::RecoveryRequired
            );
            assert_eq!(writer.requests.lock().unwrap().len(), 1, "lookup is read-only");
        }
        let response = restored.workbench_command(actor(), &resume).await;
        assert!(matches!(response, AppResponsePayload::WorkbenchReceipt(_)), "{response:?}");
        assert_eq!(
            wait_for_terminal(&restored, run).await.phase(),
            ProductRunPhase::WaitingForUser
        );
        assert_eq!(
            (goal(&restored, workspace).attempt(), goal(&restored, workspace).usage().requests()),
            (2, 2)
        );
        assert_eq!(restored.workbench_command(actor(), &resume).await, response);
        assert_eq!(writer.requests.lock().unwrap().len(), 2);
        restored.shutdown().await.expect("shutdown product runs");
    });
}

#[test]
fn failed_resume_projection_does_not_publish_an_in_memory_launch_or_call_provider() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().unwrap();
        let writer = scripted(
            0xb1,
            "failed-resume-writer",
            vec![
                support::text_response(b"First answer."),
                support::text_response(b"Recovered answer."),
            ],
        );
        let reviewer = scripted(0xb2, "failed-resume-reviewer", Vec::new());
        let fixer = scripted(0xb3, "failed-resume-fixer", Vec::new());
        let workspace = WorkspaceId::new([0xb4; 16]).unwrap();
        let run = RunId::new([0xb5; 16]).unwrap();
        let original =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        start_goal(&original, workspace, run, [&writer, &reviewer, &fixer]).await;
        wait_for_terminal(&original, run).await;
        pause_idle(&original, workspace).await;
        let resume = resume_command(&original, workspace);
        inject_persistence_fault(
            &original.inner.directory,
            run,
            PersistenceFaultPoint::BeforeRename,
        );
        assert!(matches!(
            original.workbench_command(actor(), &resume).await,
            AppResponsePayload::Error(_)
        ));
        {
            let records = original.inner.records.read().unwrap();
            assert_eq!(records[&run].snapshot.phase(), ProductRunPhase::WaitingForUser);
            assert_eq!(records[&run].attempt_admission, None);
        }
        assert_eq!(writer.requests.lock().unwrap().len(), 1);
        original.shutdown().await.expect("shutdown product runs");
        drop(original);
        let restored =
            restore(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        for _ in 0..3 {
            assert!(matches!(
                restored.workbench_receipt(actor(), &resume),
                AppResponsePayload::WorkbenchReceipt(_)
            ));
            assert_eq!(
                restored.inner.records.read().unwrap()[&run].snapshot.phase(),
                ProductRunPhase::WaitingForUser
            );
            let admitted = goal(&restored, workspace);
            assert_eq!(admitted.state(), WorkbenchGoalState::Active);
            assert_eq!((admitted.attempt(), admitted.usage().requests()), (2, 1));
            assert_eq!(writer.requests.lock().unwrap().len(), 1, "lookup is read-only");
        }
        let response = restored.workbench_command(actor(), &resume).await;
        assert!(matches!(response, AppResponsePayload::WorkbenchReceipt(_)), "{response:?}");
        wait_for_terminal(&restored, run).await;
        assert_eq!(
            (goal(&restored, workspace).attempt(), writer.requests.lock().unwrap().len()),
            (2, 2)
        );
        restored.shutdown().await.expect("shutdown product runs");
    });
}

#[test]
fn resume_projection_requires_current_format_and_exact_receipt() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().unwrap();
        let writer = scripted(
            0xc1,
            "marker-writer",
            vec![
                support::text_response(b"First answer."),
                support::text_response(b"Resumed answer."),
            ],
        );
        let reviewer = scripted(0xc2, "marker-reviewer", Vec::new());
        let fixer = scripted(0xc3, "marker-fixer", Vec::new());
        let workspace = WorkspaceId::new([0xc4; 16]).unwrap();
        let run = RunId::new([0xc5; 16]).unwrap();
        let original =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        start_goal(&original, workspace, run, [&writer, &reviewer, &fixer]).await;
        wait_for_terminal(&original, run).await;
        pause_idle(&original, workspace).await;
        let resume = resume_command(&original, workspace);
        assert!(matches!(
            original.workbench_command(actor(), &resume).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        wait_for_terminal(&original, run).await;
        original.shutdown().await.expect("shutdown product runs");
        drop(original);
        let root = state.path().join("workbench-v1");
        let path = fs::read_dir(root.join("runs"))
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .find(|path| path.extension().is_some_and(|extension| extension == "json"))
            .unwrap();
        let saved: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert!(saved.get("goal_resume").is_some());
        let controls = crate::product_control::ControlStore::open(
            &root,
            peritus_journal::StoreId::new([0x7f; 16]).unwrap(),
        )
        .unwrap();
        for marker in [0, 10, 0xff] {
            let mut corrupt = saved.clone();
            corrupt["goal_resume"] = serde_json::json!(vec![marker; 16]);
            fs::write(&path, serde_json::to_vec(&corrupt).unwrap()).unwrap();
            let loaded =
                crate::product_run::persistence::load_workbench_records(&root, Some(&controls))
                    .unwrap();
            assert!(!loaded.contains_key(&run), "forged marker {marker} must be isolated");
        }
        let mut previous = saved;
        previous["format_version"] = serde_json::Value::from(2);
        fs::write(&path, serde_json::to_vec(&previous).unwrap()).unwrap();
        assert!(
            !crate::product_run::persistence::load_workbench_records(&root, Some(&controls))
                .unwrap()
                .contains_key(&run),
            "a previous-format projection must be isolated"
        );
        assert_eq!(writer.requests.lock().unwrap().len(), 2, "recovery reads never launch work");
    });
}

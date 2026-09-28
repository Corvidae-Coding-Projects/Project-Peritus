//! Durable task preimages survive sidecar loss without replacing the user's prior edits.
use super::*;

#[test]
fn completed_discard_is_recovered_after_result_persistence_failure_and_restart() {
    interaction::block_on(async {
        use super::super::persistence::{PersistenceFaultPoint, inject_persistence_fault};
        let repository = repository();
        let state = tempfile::tempdir().unwrap();
        let original = fs::read(repository.path().join("src/lib.rs")).unwrap();
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
        assert_eq!(wait_for_terminal(&running, run).await.phase(), ProductRunPhase::Complete);
        let directory = running.inner.directory.clone();
        inject_persistence_fault(&directory, run, PersistenceFaultPoint::BeforeRename);
        assert!(
            running
                .control(ProductRunControl::new(run, ProductRunControlAction::Discard))
                .await
                .is_err()
        );
        assert_eq!(fs::read(repository.path().join("src/lib.rs")).unwrap(), original);
        let mut records = super::super::load_records(&directory).unwrap();
        assert!(!records[&run].snapshot.deliverable().unwrap().discarded());
        // A new user edit after completed discard must survive recovery of the acknowledgement.
        fs::write(repository.path().join("src/lib.rs"), b"new user draft after discard\n").unwrap();
        super::super::recovery::reconcile_restored_candidates(
            &directory,
            &mut records,
            &BTreeMap::from([(workspace, repository.path().to_path_buf())]),
        )
        .unwrap();
        assert!(
            records[&run].snapshot.deliverable().unwrap().discarded(),
            "completed discard must not become a stale blocked candidate after restart"
        );
        assert_eq!(
            fs::read(repository.path().join("src/lib.rs")).unwrap(),
            b"new user draft after discard\n"
        );
        assert!(
            super::super::load_records(&directory).unwrap()[&run]
                .snapshot
                .deliverable()
                .unwrap()
                .discarded()
        );
        running.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn repeated_deliverable_actions_retry_failed_persistence_before_reporting_success() {
    interaction::block_on(async {
        use super::super::persistence::{
            PersistenceFaultPoint, clear_persistent_persistence_fault,
            inject_persistent_persistence_fault,
        };
        let repository = repository();
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
        assert_eq!(wait_for_terminal(&running, run).await.phase(), ProductRunPhase::Complete);
        for action in [
            ProductRunControlAction::Accept,
            ProductRunControlAction::Export,
            ProductRunControlAction::Discard,
        ] {
            let point = PersistenceFaultPoint::BeforeRename;
            inject_persistent_persistence_fault(&running.inner.directory, run, point);
            assert!(running.control(ProductRunControl::new(run, action)).await.is_err());
            let repeated = running.control(ProductRunControl::new(run, action)).await;
            clear_persistent_persistence_fault(&running.inner.directory, run, point);
            assert!(
                repeated.is_err(),
                "{action:?} must not report success while its result still cannot be saved"
            );
            let confirmed = running.control(ProductRunControl::new(run, action)).await.unwrap();
            let restored = super::super::load_records(&running.inner.directory).unwrap();
            assert_eq!(restored[&run].snapshot.deliverable(), confirmed.deliverable());
        }
        running.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn restarted_deliverable_exports_and_discards_from_embedded_preimages() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let original = b"// User's uncommitted note.\npub fn answer() -> i32 { 0 }\n";
        fs::write(repository.path().join("src/lib.rs"), original).expect("prior user edit");
        fs::write(repository.path().join("user-draft.txt"), b"keep this draft\n").expect("draft");
        let writer = scripted(0x21, "writer", complete_writer(CORRECT));
        let reviewer = scripted(0x22, "reviewer", clean_review());
        let fixer = scripted(0x23, "fixer", Vec::new());
        let workspace = WorkspaceId::new([0x24; 16]).expect("workspace");
        let run = RunId::new([0x25; 16]).expect("run");
        let running =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        let request = ProductRunRequest::new(
            run,
            workspace,
            ProductProviderSelection::new(
                writer.profile.profile_id(),
                reviewer.profile.profile_id(),
                fixer.profile.profile_id(),
            ),
            "Return 42 with verified tests.".to_owned(),
        )
        .expect("request");
        running.start(request).await.expect("start");
        let terminal = wait_for_terminal(&running, run).await;
        assert_eq!(terminal.phase(), ProductRunPhase::Complete, "{}", terminal.summary());
        let baseline = {
            let records = running.inner.records.read().expect("records");
            let record = records.get(&run).expect("record");
            assert!(record.task_baseline_required);
            record.task_baseline.clone().expect("embedded preimages")
        };
        let directory = running.inner.directory.clone();
        running.shutdown(Duration::from_secs(5)).await;
        drop(running);
        let sidecars = fs::read_dir(&directory)
            .expect("directory")
            .map(|entry| entry.expect("entry").path())
            .filter(|path| path.extension().is_some_and(|extension| extension == "baseline"))
            .collect::<Vec<_>>();
        assert_eq!(sidecars.len(), 1);
        fs::remove_file(&sidecars[0]).expect("simulate lost baseline sidecar");
        let records = super::super::load_records(&directory).expect("restore records");
        assert_eq!(
            records.get(&run).expect("record").task_baseline.as_deref(),
            Some(baseline.as_str())
        );
        let restarted =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        *restarted.inner.records.write().expect("records") = records;
        let exported = restarted
            .control(ProductRunControl::new(run, ProductRunControlAction::Export))
            .await
            .expect("export from durable preimages");
        let patch = fs::read_to_string(exported.deliverable().expect("deliverable").export_path())
            .expect("patch");
        assert!(patch.contains("-// User's uncommitted note."), "{patch}");
        assert!(!patch.contains("user-draft.txt"), "unrelated draft excluded");
        let damaged_receipt = directory
            .join(format!("{:032x}.discard-result", u128::from_be_bytes(run.into_bytes())));
        fs::write(&damaged_receipt, b"damaged recovery record\n").unwrap();
        let repeated_export =
            restarted.control(ProductRunControl::new(run, ProductRunControlAction::Export)).await;
        fs::remove_file(damaged_receipt).unwrap();
        assert_eq!(
            repeated_export
                .expect(
                    "an existing saved patch remains available independently of discard recovery"
                )
                .deliverable(),
            exported.deliverable()
        );
        let export_path = exported.deliverable().unwrap().export_path();
        fs::remove_file(export_path).unwrap();
        restarted
            .control(ProductRunControl::new(run, ProductRunControlAction::Export))
            .await
            .expect("re-export a lost patch while the exact candidate is still available");
        assert_eq!(fs::read_to_string(export_path).expect("replacement export exists"), patch);
        restarted
            .control(ProductRunControl::new(run, ProductRunControlAction::Discard))
            .await
            .expect("discard from durable preimages");
        assert_eq!(fs::read(repository.path().join("src/lib.rs")).expect("preimage"), original);
        assert_eq!(
            fs::read(repository.path().join("user-draft.txt")).expect("draft"),
            b"keep this draft\n"
        );
        restarted.shutdown(Duration::from_secs(5)).await;
    });
}

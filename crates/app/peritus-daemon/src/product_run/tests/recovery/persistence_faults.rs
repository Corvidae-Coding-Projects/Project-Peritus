//! Persistence fault and shutdown-race recovery regressions.

use super::*;

#[test]
fn repeated_failed_recovery_admission_does_not_duplicate_restart_narration() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = stalled(0xa1, "writer");
        let reviewer = scripted(0xa2, "reviewer", clean_review());
        let fixer = scripted(0xa3, "fixer", Vec::new());
        let run_id = RunId::new([0xa4; 16]).expect("run");
        let workspace_id = WorkspaceId::new([0xa5; 16]).expect("workspace");
        let running =
            service(state.path(), repository.path(), workspace_id, [&writer, &reviewer, &fixer]);
        let request = ProductRunRequest::new(
            run_id,
            workspace_id,
            ProductProviderSelection::new(
                writer.profile.profile_id(),
                reviewer.profile.profile_id(),
                fixer.profile.profile_id(),
            ),
            "Exercise failed restart admission.".to_owned(),
        )
        .expect("request");
        running.start(request).await.expect("start writer");
        tokio::time::timeout(Duration::from_secs(5), async {
            while writer.requests.lock().expect("writer requests").is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("provider boundary");
        running.shutdown(Duration::from_secs(5)).await;
        let records =
            super::super::super::load_records(&running.inner.directory).expect("restore runs");
        drop(running);

        let unavailable_workspace = WorkspaceId::new([0xa6; 16]).expect("other workspace");
        let restarted = service(
            state.path(),
            repository.path(),
            unavailable_workspace,
            [&writer, &reviewer, &fixer],
        );
        *restarted.inner.records.write().expect("run ownership") = records;
        restarted.resume_interrupted().await;
        restarted.resume_interrupted().await;

        let messages = restarted
            .inner
            .records
            .read()
            .expect("records")
            .get(&run_id)
            .expect("run")
            .conversation
            .messages()
            .expect("conversation");
        let recovery_messages = messages
            .iter()
            .filter(|message| {
                message.content()
                    == "The daemon restarted; I am continuing this goal from its preserved workspace."
            })
            .count();
        assert_eq!(recovery_messages, 1);
        assert_eq!(writer.requests.lock().expect("writer requests").len(), 1);
        restarted.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn product_record_fault_boundaries_preserve_an_old_or_complete_new_record() {
    interaction::block_on(async {
        use super::super::super::persistence::{PersistenceFaultPoint, inject_persistence_fault};
        use super::super::super::{ProductRunServiceError, persist_record, replace_snapshot};

        let mut points = vec![
            (PersistenceFaultPoint::BeforeWrite, false),
            (PersistenceFaultPoint::BeforeFileSync, false),
            (PersistenceFaultPoint::BeforeRename, false),
            (PersistenceFaultPoint::AfterRename, true),
        ];
        #[cfg(unix)]
        points.push((PersistenceFaultPoint::BeforeDirectorySync, true));

        for (index, (point, new_record_visible)) in points.into_iter().enumerate() {
            let repository = repository();
            let state = tempfile::tempdir().expect("state");
            let seed = u8::try_from(index).expect("fault index");
            let writer = stalled(0xb1 + seed, "writer");
            let reviewer = scripted(0xc1 + seed, "reviewer", clean_review());
            let fixer = scripted(0xd1 + seed, "fixer", Vec::new());
            let run_id = RunId::new([0xb8 + seed; 16]).expect("run");
            let workspace_id = WorkspaceId::new([0xc8 + seed; 16]).expect("workspace");
            let service = service(
                state.path(),
                repository.path(),
                workspace_id,
                [&writer, &reviewer, &fixer],
            );
            let request = ProductRunRequest::new(
                run_id,
                workspace_id,
                ProductProviderSelection::new(
                    writer.profile.profile_id(),
                    reviewer.profile.profile_id(),
                    fixer.profile.profile_id(),
                ),
                "Exercise one product-record persistence boundary.".to_owned(),
            )
            .expect("request");
            service.start(request).await.expect("persist baseline");
            tokio::time::timeout(Duration::from_secs(5), async {
                while writer.requests.lock().expect("writer requests").is_empty() {
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("provider boundary");

            let run_hex = run_id.as_bytes().iter().fold(String::new(), |mut value, byte| {
                use core::fmt::Write as _;
                let _ = write!(value, "{byte:02x}");
                value
            });
            let record_path = service.inner.directory.join(format!("{run_hex}.json"));
            let baseline: serde_json::Value =
                serde_json::from_slice(&fs::read(&record_path).expect("baseline bytes"))
                    .expect("baseline record");
            let baseline_status = baseline["status"].as_str().expect("baseline status").to_owned();
            {
                let mut records = service.inner.records.write().expect("records");
                let record = records.get_mut(&run_id).expect("run");
                record.snapshot = replace_snapshot(
                    &record.snapshot,
                    record.snapshot.phase(),
                    "fault-boundary-new-status",
                    record.snapshot.summary(),
                )
                .expect("replacement snapshot");
                inject_persistence_fault(run_id, point);
                assert!(matches!(
                    persist_record(&service.inner.directory, record),
                    Err(ProductRunServiceError::Unavailable)
                ));
            }
            let observed: serde_json::Value =
                serde_json::from_slice(&fs::read(&record_path).expect("canonical record bytes"))
                    .expect("canonical record stays complete JSON");
            let expected =
                if new_record_visible { "fault-boundary-new-status" } else { &baseline_status };
            assert_eq!(observed["status"].as_str(), Some(expected), "fault point {point:?}");
            service.shutdown(Duration::from_secs(5)).await;
        }
    });
}

#[test]
fn explicit_cancel_racing_shutdown_remains_cancelled_in_durable_state() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = stalled(0xe1, "writer");
        let reviewer = scripted(0xe2, "reviewer", clean_review());
        let fixer = scripted(0xe3, "fixer", Vec::new());
        let run_id = RunId::new([0xe4; 16]).expect("run");
        let workspace_id = WorkspaceId::new([0xe5; 16]).expect("workspace");
        let running =
            service(state.path(), repository.path(), workspace_id, [&writer, &reviewer, &fixer]);
        let durable_directory = running.inner.directory.clone();
        let request = ProductRunRequest::new(
            run_id,
            workspace_id,
            ProductProviderSelection::new(
                writer.profile.profile_id(),
                reviewer.profile.profile_id(),
                fixer.profile.profile_id(),
            ),
            "Hold this run at the provider boundary.".to_owned(),
        )
        .expect("request");
        running.start(request).await.expect("start writer");

        tokio::time::timeout(Duration::from_secs(5), async {
            while writer.requests.lock().expect("writer requests").is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("writer reached the provider boundary");
        running.cancel(run_id).expect("explicit user stop");
        running.shutdown(Duration::from_secs(5)).await;
        drop(running);

        let records =
            super::super::super::load_records(&durable_directory).expect("reload durable runs");
        assert_eq!(
            records.get(&run_id).expect("durable run").snapshot.phase(),
            ProductRunPhase::Cancelled,
            "shutdown must not convert an explicitly cancelled run into restartable work",
        );
        assert_eq!(writer.requests.lock().expect("writer requests").len(), 1);
    });
}

//! Explicit cancellation remains durable across daemon shutdown.

use super::*;

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
        running.shutdown().await.expect("shutdown product runs");
        let records = running.load_test_records().expect("reload durable runs");
        drop(running);
        assert_eq!(
            records.get(&run_id).expect("durable run").snapshot.phase(),
            ProductRunPhase::Cancelled,
            "shutdown must not convert an explicitly cancelled run into restartable work",
        );
        assert_eq!(writer.requests.lock().expect("writer requests").len(), 1);
    });
}

#[test]
fn shutdown_reports_durable_projection_failure() {
    interaction::block_on(async {
        use super::super::super::persistence::{
            PersistenceFaultPoint, clear_persistent_persistence_fault,
            inject_persistent_persistence_fault,
        };

        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = stalled(0xf1, "writer");
        let reviewer = scripted(0xf2, "reviewer", clean_review());
        let fixer = scripted(0xf3, "fixer", Vec::new());
        let run_id = RunId::new([0xf4; 16]).expect("run");
        let workspace_id = WorkspaceId::new([0xf5; 16]).expect("workspace");
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
            "Report a shutdown durability failure.".to_owned(),
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

        inject_persistent_persistence_fault(
            &running.inner.directory,
            run_id,
            PersistenceFaultPoint::BeforeWrite,
        );
        let error = running.shutdown().await.expect_err("shutdown durability must be reported");
        clear_persistent_persistence_fault(
            &running.inner.directory,
            run_id,
            PersistenceFaultPoint::BeforeWrite,
        );

        assert_eq!(error.code_kind(), crate::DaemonErrorCode::Storage);
        assert_eq!(error.operation(), "settle product-run shutdown");
        assert!(error.detail().contains("durable write failure"));
    });
}

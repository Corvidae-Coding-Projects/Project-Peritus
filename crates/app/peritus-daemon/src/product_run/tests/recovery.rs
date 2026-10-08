//! Automatic continuation after a daemon interruption, with preserved completed effects.

use super::*;

#[path = "recovery/persistence_faults.rs"]
mod persistence_faults;
#[path = "recovery/restart.rs"]
mod restart;
#[path = "recovery/shutdown.rs"]
mod shutdown;

#[test]
fn dropped_preparation_results_retire_only_their_unpublished_attempt_owner() {
    interaction::block_on(async {
        use peritus_app_protocol::{
            AppResponsePayload, ControlOperationId, ConversationId as AppConversationId,
            ConversationTitle, ProductInteractionMode, ProductRoleModels, WorkbenchCommand,
            WorkbenchInputId, WorkbenchInputOrder, WorkbenchInputText, WorkbenchIntent,
            WorkbenchNewInput, WorkbenchQuery, WorkbenchQueueIntent,
        };
        use peritus_product_runner::control::{
            ControlIntent, ControlOperation, ConversationId, OperationId,
        };
        use peritus_types::ActorId;

        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = stalled(0x21, "writer");
        let reviewer = scripted(0x22, "reviewer", clean_review());
        let fixer = scripted(0x23, "fixer", Vec::new());
        let run_id = RunId::new([0x24; 16]).expect("run");
        let workspace_id = WorkspaceId::new([0x25; 16]).expect("workspace");
        let service =
            service(state.path(), repository.path(), workspace_id, [&writer, &reviewer, &fixer]);
        let providers = ProductProviderSelection::new(
            writer.profile.profile_id(),
            reviewer.profile.profile_id(),
            fixer.profile.profile_id(),
        );
        let actor = ActorId::new([0x26; 16]).expect("actor");
        let conversation = [0x27; 16];
        let query = WorkbenchQuery::new(
            AppConversationId::new(conversation).expect("conversation"),
            workspace_id,
        );
        let created = WorkbenchCommand::new(
            ControlOperationId::new([0x28; 16]).expect("create operation"),
            query,
            0,
            WorkbenchIntent::CreateConversation(
                ConversationTitle::new("prepared owner regression".to_owned()).expect("title"),
            ),
        );
        assert!(matches!(
            service.workbench_command(actor, &created).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let queued = WorkbenchCommand::new(
            ControlOperationId::new([0x29; 16]).expect("queue operation"),
            query,
            1,
            WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(
                WorkbenchNewInput::new(
                    WorkbenchInputId::new([0x2a; 16]).expect("input"),
                    WorkbenchInputText::new("Retain exact launch ownership.".to_owned())
                        .expect("input text"),
                    WorkbenchInputOrder::new(Vec::new()).expect("input order"),
                )
                .expect("queued input"),
            )),
        );
        assert!(matches!(
            service.workbench_command(actor, &queued).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let request = ProductRunRequest::new(
            run_id,
            workspace_id,
            providers,
            "Execute the selected durable workbench inputs.".to_owned(),
        )
        .expect("request");
        let control = ControlOperation::new(
            OperationId::new([0x2b; 16]).expect("start operation"),
            ConversationId::new(conversation).expect("domain conversation"),
            actor,
            workspace_id,
            2,
            ControlIntent::StartExecution {
                run: run_id.into_bytes(),
                settings_digest: [0x2c; 32],
            },
        );
        let options = interaction::InteractionOptions::new(
            control,
            ProductInteractionMode::Build,
            ProductRoleModels::default(),
        );

        let first_barrier =
            super::super::ownership::inject_prepared_owner_barrier(run_id);
        let first_service = service.clone();
        let first = tokio::spawn(async move {
            first_service.start_configured(request, options).await
        });
        tokio::time::timeout(Duration::from_secs(5), first_barrier.reached())
            .await
            .expect("first preparation registered its owner");
        first.abort();
        let _ = first.await;
        first_barrier.release();
        wait_for_unpublished_owner_retirement(&service, run_id).await;

        let recovery_barrier =
            super::super::ownership::inject_prepared_owner_barrier(run_id);
        let recovery_service = service.clone();
        let recovery = tokio::spawn(async move {
            recovery_service.recover_queued_launch(run_id).await
        });
        tokio::time::timeout(Duration::from_secs(5), recovery_barrier.reached())
            .await
            .expect("recovery preparation registered its owner");
        recovery.abort();
        let _ = recovery.await;
        recovery_barrier.release();
        wait_for_unpublished_owner_retirement(&service, run_id).await;

        let snapshot = service
            .recover_queued_launch(run_id)
            .await
            .expect("a dropped prepared owner must not strand PendingLaunch");
        assert_eq!(snapshot.phase(), ProductRunPhase::Queued);
        tokio::time::timeout(Duration::from_secs(5), async {
            while writer.requests.lock().expect("writer requests").is_empty() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("published recovery reached the provider");
        {
            let owners = service
                .inner
                .run_cancellations
                .lock()
                .expect("run cancellations");
            let owner = owners.get(&run_id).expect("published run owner");
            assert!(owner.active.load(std::sync::atomic::Ordering::Acquire));
            assert!(owner.launched.load(std::sync::atomic::Ordering::Acquire));
        }
        service.cancel(run_id).expect("cancel stalled recovery");
        service.shutdown().await.expect("shutdown product runs");
    });
}

async fn wait_for_unpublished_owner_retirement(service: &ProductRunService, run_id: RunId) {
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let retired = service
                .inner
                .run_cancellations
                .lock()
                .expect("run cancellations")
                .get(&run_id)
                .is_some_and(|owner| {
                    !owner.active.load(std::sync::atomic::Ordering::Acquire)
                        && !owner.launched.load(std::sync::atomic::Ordering::Acquire)
                });
            if retired {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("unpublished prepared owner retired after its result was dropped");
}

#[test]
fn accepted_result_stays_complete_when_shutdown_follows_late_cancellation() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(0x41, "writer", complete_writer(CORRECT));
        let reviewer = scripted(0x42, "reviewer", clean_review());
        let fixer = scripted(0x43, "fixer", Vec::new());
        let run_id = RunId::new([0x44; 16]).expect("run");
        let workspace_id = WorkspaceId::new([0x45; 16]).expect("workspace");
        let running =
            service(state.path(), repository.path(), workspace_id, [&writer, &reviewer, &fixer]);
        let barrier = super::super::execution::inject_finish_barrier(run_id);
        let request = ProductRunRequest::new(
            run_id,
            workspace_id,
            ProductProviderSelection::new(
                writer.profile.profile_id(),
                reviewer.profile.profile_id(),
                fixer.profile.profile_id(),
            ),
            "Add a tested answer function that returns 42.".to_owned(),
        )
        .expect("request");

        running.start(request).await.expect("start run");
        tokio::time::timeout(Duration::from_secs(5), barrier.reached())
            .await
            .expect("runner reached finalization barrier");
        let cancelling = running.cancel(run_id).expect("request late cancellation");
        assert!(!cancelling.phase().terminal());
        let shutdown_service = running.clone();
        let shutdown = tokio::spawn(async move {
            shutdown_service.shutdown().await.expect("shutdown product runs");
        });
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let stopping =
                    running.inner.records.read().expect("run records").get(&run_id).is_some_and(
                        |record| {
                            record.snapshot.status()
                                == "Stopping safely after the current effect boundary"
                        },
                    );
                if stopping {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("shutdown reached its task-drain boundary");
        barrier.release();
        shutdown.await.expect("shutdown task");

        let live = wait_for_terminal(&running, run_id).await;
        assert_eq!(live.phase(), ProductRunPhase::Complete);
        let restored = running.load_test_records().expect("reload runs");
        assert_eq!(
            restored.get(&run_id).expect("restored run").snapshot.phase(),
            live.phase(),
            "a persisted terminal result must not change when reopened",
        );
    });
}

#[test]
#[ignore = "subprocess fixture; invoked by abrupt_cancel_survives_process_termination"]
fn abrupt_cancel_child_fixture() {
    let Ok(state) = std::env::var("PERITUS_ABRUPT_CANCEL_STATE") else { return };
    let repository = std::env::var("PERITUS_ABRUPT_CANCEL_REPOSITORY").expect("repository path");
    let barrier = std::env::var("PERITUS_ABRUPT_CANCEL_BARRIER").expect("barrier path");
    interaction::block_on(async {
        let writer = stalled(0xf1, "writer");
        let reviewer = scripted(0xf2, "reviewer", clean_review());
        let fixer = scripted(0xf3, "fixer", Vec::new());
        let run_id = RunId::new([0xf4; 16]).expect("run");
        let workspace_id = WorkspaceId::new([0xf5; 16]).expect("workspace");
        let running = service(
            std::path::Path::new(&state),
            std::path::Path::new(&repository),
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
            "Remain stopped after an abrupt daemon death.".to_owned(),
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
        running.cancel(run_id).expect("persist explicit cancellation");
        fs::write(&barrier, b"cancel-persisted\n").expect("publish crash barrier");
        std::future::pending::<()>().await;
    });
}

#[test]
fn abrupt_cancel_survives_process_termination() {
    struct OwnedChild(std::process::Child);
    impl Drop for OwnedChild {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let barrier = state.path().join("cancel-persisted.barrier");
        let executable = std::env::current_exe().expect("current test executable");
        let child = std::process::Command::new(executable)
            .args([
                "--exact",
                "product_run::tests::recovery::abrupt_cancel_child_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("PERITUS_ABRUPT_CANCEL_STATE", state.path())
            .env("PERITUS_ABRUPT_CANCEL_REPOSITORY", repository.path())
            .env("PERITUS_ABRUPT_CANCEL_BARRIER", &barrier)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn owned crash fixture");
        let mut child = OwnedChild(child);
        tokio::time::timeout(Duration::from_secs(10), async {
            while !barrier.exists() {
                assert!(
                    child.0.try_wait().expect("inspect child").is_none(),
                    "fixture exited early"
                );
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("fixture reached durable cancellation barrier");
        child.0.kill().expect("terminate exact owned fixture");
        let status = child.0.wait().expect("reap exact owned fixture");
        assert!(!status.success());

        let writer = stalled(0xf1, "writer");
        let reviewer = scripted(0xf2, "reviewer", clean_review());
        let fixer = scripted(0xf3, "fixer", Vec::new());
        let run_id = RunId::new([0xf4; 16]).expect("run");
        let workspace_id = WorkspaceId::new([0xf5; 16]).expect("workspace");
        let restarted =
            service(state.path(), repository.path(), workspace_id, [&writer, &reviewer, &fixer]);
        let records = restarted.load_test_records().expect("reload runs");
        assert_eq!(
            records.get(&run_id).expect("durable run").snapshot.phase(),
            ProductRunPhase::Cancelled,
        );
        *restarted.inner.records.write().expect("restore ownership") = records;
        assert!(writer.requests.lock().expect("no restarted requests").is_empty());
        restarted.shutdown().await.expect("shutdown product runs");
    });
}

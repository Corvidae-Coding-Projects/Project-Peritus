//! Real admission, idle continuation, duplicate continuation, and authorization.
use super::*;
use peritus_app_protocol::{
    DoctorQuery, DoctorStatus, ProductRunControl, ProductRunControlAction, ProductRunOperationKind,
    ProductRunOperationState,
};

#[test]
fn pending_input_waits_for_unknown_command_reconciliation_before_provider_resume() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(
            0x21,
            "chat",
            vec![
                support::text_response(b"Initial answer."),
                support::text_response(b"Reconciled answer."),
            ],
        );
        let reviewer = scripted(0x22, "review", Vec::new());
        let fixer = scripted(0x23, "fix", Vec::new());
        let workspace = WorkspaceId::new([0x24; 16]).expect("workspace");
        let run = RunId::new([0x25; 16]).expect("run");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&service, workspace).await;
        let initial = start(workspace, run, [&writer, &reviewer, &fixer]);
        assert!(matches!(
            service.workbench_command(actor(), &initial).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let terminal = wait_for_terminal(&service, run).await;
        let AppResponsePayload::WorkbenchExecution(observed) =
            service.workbench_execution(actor(), query(workspace))
        else {
            panic!("workbench execution")
        };
        let input = command(
            workspace,
            88,
            observed.snapshot().revision(),
            WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(
                WorkbenchNewInput::new(
                    WorkbenchInputId::new([88; 16]).expect("input"),
                    WorkbenchInputText::new("CONTINUE_AFTER_RECONCILIATION".to_owned())
                        .expect("text"),
                    WorkbenchInputOrder::new(Vec::new()).expect("order"),
                )
                .expect("input"),
            )),
        );
        assert!(matches!(
            service.workbench_command(actor(), &input).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        {
            let mut records = service.inner.records.write().expect("records");
            let record = records.get_mut(&run).expect("record");
            record.snapshot = crate::product_run::snapshot::replace_snapshot(
                &terminal,
                ProductRunPhase::RecoveryRequired,
                "Interrupted after command admission",
                terminal.summary(),
            )
            .expect("recovery snapshot");
            record.interruption_cause = "Command completion was not observed.".to_owned();
            crate::product_run::persistence::persist_record(&service.inner.directory, record)
                .expect("persist recovery state");
        }
        let effects = service
            .inner
            .directory
            .join(format!("{:032x}.trace", u128::from_be_bytes(run.into_bytes())))
            .with_extension("effects.bin");
        let scope = format!(
            "peritus-{:032x}-writer-1-revision-1-invocation-1-test",
            u128::from_be_bytes(run.into_bytes())
        );
        support::write_command_receipt(
            &effects,
            &scope,
            2,
            Some(support::exact_command_owner_fixture(run)),
            true,
        );

        let continuation = peritus_app_protocol::WorkbenchContinuation::new(
            query(workspace),
            ProductInteractionMode::Chat,
        );
        let AppResponsePayload::Interaction(blocked) =
            service.continue_workbench_execution(actor(), continuation).await
        else {
            panic!("blocked continuation observation")
        };
        assert_eq!(blocked.snapshot().operation().kind(), ProductRunOperationKind::Command);
        assert_eq!(
            blocked.snapshot().operation().state(),
            ProductRunOperationState::OutcomeUnknown
        );
        assert_eq!(writer.requests.lock().expect("requests").len(), 1);
        let diagnostics = service
            .doctor(DoctorQuery::new(workspace, Some(writer.profile.profile_id())))
            .expect("diagnostics project the operation owner");
        assert!(diagnostics.findings().iter().any(|finding| {
            finding.check() == "workspace-activity"
                && finding.status() == DoctorStatus::Warning
                && finding.observation().contains("requiring explicit reconciliation")
        }));

        service
            .control(ProductRunControl::new(run, ProductRunControlAction::Acknowledge))
            .await
            .expect("acknowledge unknown command");
        assert!(matches!(
            service.continue_workbench_execution(actor(), continuation).await,
            AppResponsePayload::Interaction(_)
        ));
        wait_for_terminal(&service, run).await;
        {
            let requests = writer.requests.lock().expect("requests");
            assert_eq!(requests.len(), 2);
            let second = requests[1].canonical_bytes().expect("request bytes");
            assert!(
                second
                    .windows(b"CONTINUE_AFTER_RECONCILIATION".len())
                    .any(|bytes| { bytes == b"CONTINUE_AFTER_RECONCILIATION" })
            );
        }
        service.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn selected_conversation_continues_receipted_input_without_legacy_admission() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer = scripted(
            0x31,
            "chat",
            vec![
                support::text_response(b"First answer."),
                support::text_response(b"Revised answer."),
            ],
        );
        let reviewer = scripted(0x32, "review", Vec::new());
        let fixer = scripted(0x33, "fix", Vec::new());
        let workspace = WorkspaceId::new([0x34; 16]).expect("workspace");
        let run = RunId::new([0x35; 16]).expect("run");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&service, workspace).await;
        assert!(matches!(
            service.workbench_execution(ActorId::new([99; 16]).expect("actor"), query(workspace)),
            AppResponsePayload::Error(_)
        ));
        let initial = start(workspace, run, [&writer, &reviewer, &fixer]);
        assert!(matches!(
            service.workbench_command(actor(), &initial).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        wait_for_terminal(&service, run).await;
        let lookup = ProductInteractionQuery::new(run);
        assert!(
            service.query_interaction_binding(ActorId::new([99; 16]).unwrap(), lookup).is_err()
        );
        let reopened =
            service.query_interaction_binding(actor(), lookup).expect("exact run binding");
        assert_eq!(reopened.conversation(), query(workspace));
        assert_eq!(reopened.interaction().snapshot().run_id(), run);
        assert!(
            reopened.interaction().snapshot().phase().terminal(),
            "opening must not resume work"
        );
        let AppResponsePayload::WorkbenchExecution(observed) =
            service.workbench_execution(actor(), query(workspace))
        else {
            panic!("binding")
        };
        assert_eq!(observed.run(), Some(run));
        let input = command(
            workspace,
            88,
            observed.snapshot().revision(),
            WorkbenchIntent::Queue(WorkbenchQueueIntent::Enqueue(
                WorkbenchNewInput::new(
                    WorkbenchInputId::new([88; 16]).expect("id"),
                    WorkbenchInputText::new("CHANGED_MY_MIND: use Celsius instead.".to_owned())
                        .expect("text"),
                    WorkbenchInputOrder::new(Vec::new()).expect("order"),
                )
                .expect("input"),
            )),
        );
        let accepted = service.workbench_command(actor(), &input).await;
        assert!(matches!(accepted, AppResponsePayload::WorkbenchReceipt(_)));
        assert_eq!(service.workbench_command(actor(), &input).await, accepted);
        assert!(matches!(
            service
                .continue_workbench_execution(
                    ActorId::new([99; 16]).expect("actor"),
                    peritus_app_protocol::WorkbenchContinuation::new(
                        query(workspace),
                        ProductInteractionMode::Plan
                    )
                )
                .await,
            AppResponsePayload::Error(_)
        ));
        let continued = service
            .continue_workbench_execution(
                actor(),
                peritus_app_protocol::WorkbenchContinuation::new(
                    query(workspace),
                    ProductInteractionMode::Plan,
                ),
            )
            .await;
        assert!(matches!(continued, AppResponsePayload::Interaction(_)), "{continued:?}");
        wait_for_terminal(&service, run).await;
        assert!(matches!(
            service
                .continue_workbench_execution(
                    actor(),
                    peritus_app_protocol::WorkbenchContinuation::new(
                        query(workspace),
                        ProductInteractionMode::Plan
                    )
                )
                .await,
            AppResponsePayload::Interaction(_)
        ));
        let latest = service.query_interaction(ProductInteractionQuery::new(run)).expect("mode");
        assert_eq!(
            latest.mode(),
            ProductInteractionMode::Plan,
            "changed mode is applied to the same durable conversation"
        );
        let brief = service.workbench_brief(actor(), query(workspace));
        let AppResponsePayload::WorkbenchBrief(brief) = brief else {
            panic!("multiple replies must not make the brief unreadable: {brief:?}")
        };
        assert_eq!(brief.proposals().len(), 2);
        assert!(brief.proposals().iter().any(|proposal| proposal.text() == "First answer."));
        assert!(brief.proposals().iter().any(|proposal| proposal.text() == "Revised answer."));
        let requests = writer.requests.lock().expect("requests").clone();
        assert_eq!(
            requests.len(),
            2,
            "duplicate continuation does not create another provider request"
        );
        let bytes = requests[1].canonical_bytes().expect("request");
        assert!(bytes.windows(b"CHANGED_MY_MIND".len()).any(|bytes| bytes == b"CHANGED_MY_MIND"));
        drop(requests);
        let snapshot = service
            .query_interaction(ProductInteractionQuery::new(run))
            .expect("public transcript");
        assert!(
            snapshot
                .activities()
                .iter()
                .any(|activity| activity.text().contains("CHANGED_MY_MIND"))
        );
        assert!(!snapshot.activities().iter().any(|activity| activity.text() == "Execute the selected durable workbench inputs."));
        let request_id = RequestId::new([97; 16]).expect("request");
        let stop = ProductRunControl::new(run, ProductRunControlAction::Cancel);
        assert!(matches!(
            service
                .control_authenticated(ActorId::new([99; 16]).expect("actor"), request_id, stop)
                .await,
            AppResponsePayload::Error(_)
        ));
        let stopped = service.control_authenticated(actor(), request_id, stop).await;
        assert!(!matches!(stopped, AppResponsePayload::Error(_)), "{stopped:?}");
        assert_eq!(
            service
                .query_interaction(ProductInteractionQuery::new(run))
                .expect("stopped")
                .snapshot()
                .phase(),
            ProductRunPhase::Cancelled
        );
        service.shutdown(Duration::from_secs(5)).await;
    });
}

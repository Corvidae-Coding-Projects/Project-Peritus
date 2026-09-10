use super::*;
use peritus_app_protocol::{
    WorkbenchPermissionCapability, WorkbenchPermissionChange, WorkbenchPermissionProvenance,
};
use peritus_product_runner::control::{HostPermissions, PermissionCapability};

#[test]
fn narrowing_network_blocks_the_actual_provider_boundary_and_survives_restart() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer =
            scripted(0x81, "chat", vec![support::text_response(b"THIS_PROVIDER_MUST_NOT_RUN")]);
        let reviewer = scripted(0x82, "review", Vec::new());
        let fixer = scripted(0x83, "fix", Vec::new());
        let workspace = WorkspaceId::new([0x84; 16]).expect("workspace");
        let run = RunId::new([0x85; 16]).expect("run");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&service, workspace).await;

        let narrowed = command(
            workspace,
            0x86,
            3,
            WorkbenchIntent::SetPermissions(WorkbenchPermissionChange::new(
                0,
                WorkbenchPermissionCapability::Network,
                false,
            )),
        );
        let receipt = service.workbench_command(actor(), &narrowed).await;
        assert!(matches!(receipt, AppResponsePayload::WorkbenchReceipt(_)), "{receipt:?}");
        let AppResponsePayload::WorkbenchPermissions(projected) =
            service.workbench_permissions(actor(), query(workspace))
        else {
            panic!("permission projection")
        };
        assert_eq!(projected.conversation_revision(), 4);
        assert_eq!(projected.authority_revision(), 1);
        let network = projected
            .entries()
            .iter()
            .find(|entry| entry.capability() == WorkbenchPermissionCapability::Network)
            .expect("network row");
        assert!(network.host_allowed());
        assert!(!network.effective_allowed());
        assert_eq!(network.provenance(), WorkbenchPermissionProvenance::UserRestriction);

        let start = command(
            workspace,
            0x87,
            4,
            WorkbenchIntent::StartExecution(WorkbenchExecutionSettings::new(
                run,
                ProductProviderSelection::new(
                    writer.profile.profile_id(),
                    reviewer.profile.profile_id(),
                    fixer.profile.profile_id(),
                ),
                ProductInteractionMode::Chat,
                ProductRoleModels::default(),
            )),
        );
        let accepted = service.workbench_command(actor(), &start).await;
        assert!(matches!(accepted, AppResponsePayload::WorkbenchReceipt(_)), "{accepted:?}");
        let terminal = wait_for_terminal(&service, run).await;
        assert_eq!(terminal.phase(), ProductRunPhase::Failed, "{}", terminal.summary());
        assert!(writer.requests.lock().expect("requests").is_empty());

        service.shutdown(Duration::from_secs(5)).await;
        drop(service);
        let controls = crate::product_control::ControlStore::open(
            &state.path().join("workbench-v1"),
            peritus_journal::StoreId::new([0x7f; 16]).expect("store"),
        )
        .expect("restart controls");
        let restored = controls.permission_policy(workspace).expect("restored policy");
        assert_eq!(restored.revision(), 1);
        assert!(!restored.effective(HostPermissions::all(), PermissionCapability::Network));
    });
}

#[test]
fn a_permission_change_cannot_grant_above_the_host_ceiling() {
    let initial = peritus_product_runner::control::PermissionPolicy::default();
    let host = HostPermissions::all().without(PermissionCapability::Write);
    assert_eq!(
        initial.apply(0, PermissionCapability::Write, true, host),
        Err(ControlError::InvalidInput)
    );
}

#[test]
fn narrowing_process_blocks_a_direct_preview_before_receipt_or_launch() {
    interaction::block_on(async {
        let repository = repository();
        let state = tempfile::tempdir().expect("state");
        let writer =
            scripted(0x91, "chat", vec![support::text_response(b"Preview setup is ready.")]);
        let reviewer = scripted(0x92, "review", Vec::new());
        let fixer = scripted(0x93, "fix", Vec::new());
        let workspace = WorkspaceId::new([0x94; 16]).expect("workspace");
        let run = RunId::new([0x95; 16]).expect("run");
        let service =
            service(state.path(), repository.path(), workspace, [&writer, &reviewer, &fixer]);
        queue(&service, workspace).await;
        let accepted = service
            .workbench_command(actor(), &start(workspace, run, [&writer, &reviewer, &fixer]))
            .await;
        assert!(matches!(accepted, AppResponsePayload::WorkbenchReceipt(_)), "{accepted:?}");
        let _ = wait_for_terminal(&service, run).await;
        let current = match service.workbench_query(actor(), query(workspace)) {
            AppResponsePayload::Workbench(value) => value.revision(),
            value => panic!("workbench snapshot: {value:?}"),
        };
        let restrict = command(
            workspace,
            0x96,
            current,
            WorkbenchIntent::SetPermissions(WorkbenchPermissionChange::new(
                0,
                WorkbenchPermissionCapability::Process,
                false,
            )),
        );
        assert!(matches!(
            service.workbench_command(actor(), &restrict).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let current = match service.workbench_query(actor(), query(workspace)) {
            AppResponsePayload::Workbench(value) => value.revision(),
            value => panic!("workbench snapshot: {value:?}"),
        };
        let text = |value: &str| WorkbenchLaunchText::new(value.to_owned()).expect("launch text");
        let marker = repository.path().join("preview-process-started");
        let profile = WorkbenchLaunchProfile::new(
            run,
            text("sh"),
            vec![text("-c"), text("printf started > preview-process-started")],
            text("."),
            Vec::new(),
            WorkbenchLaunchSource::new(
                WorkbenchLaunchSourceKind::ManagedCandidate,
                text("."),
                peritus_product_runner::ProductRunner::candidate_digest(repository.path())
                    .expect("candidate digest"),
            ),
            None,
            2_000,
            10_000,
            false,
        )
        .expect("launch profile");
        let launch = command(workspace, 0x97, current, WorkbenchIntent::StartPreview(profile));
        let response = service.workbench_preview_local_command(actor(), &launch);
        assert!(
            matches!(response, AppResponsePayload::Error(ref error) if error.code() == peritus_app_protocol::AppErrorCode::ReadOnly),
            "{response:?}"
        );
        assert!(!marker.exists());
        assert!(service.inner.preview_processes.lock().expect("preview processes").is_empty());
        {
            let records = service.inner.records.read().expect("run records");
            assert!(records.get(&run).expect("run").preview.operations.is_empty());
        }
        service.shutdown(Duration::from_secs(5)).await;
    });
}

//! Ordinary-folder initialization through the actual session-bound committed mutation path.

use super::*;
use peritus_app_protocol::{
    InitDiscoveryRequest, InitProposal, WorkbenchPermissionCapability, WorkbenchPermissionChange,
};

fn discover(service: &ProductRunService, workspace: WorkspaceId, revision: u64) -> InitProposal {
    match service
        .discover_init(actor(), InitDiscoveryRequest::new(query(workspace), revision).unwrap())
    {
        AppResponsePayload::InitProposal(proposal) => proposal,
        response => panic!("expected initialization proposal, got {response:?}"),
    }
}

fn init_service(
    state: &std::path::Path,
    folder: &std::path::Path,
    workspace: WorkspaceId,
    writer: &Arc<ScriptedProvider>,
) -> ProductRunService {
    let mut service = service(state, folder, workspace, [writer, writer, writer]);
    let identity = peritus_workspace::FolderIdentity::observe(folder).unwrap();
    let identity_hex = hex(identity.digest().as_bytes());
    let workspace_hex = hex(workspace.as_bytes());
    let declaration = toml::from_str(&format!(
        "workspace_id = {workspace_hex:?}\nroot = {:?}\nidentity = {identity_hex:?}\nwritable = true\nprotected_paths = []\n",
        folder.to_str().unwrap(),
    )).unwrap();
    Arc::get_mut(&mut service.inner).unwrap().folders.insert(workspace, declaration);
    service
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut output, byte| {
        write!(&mut output, "{byte:02x}").expect("write to string");
        output
    })
}

#[test]
fn confirmed_init_preserves_existing_instructions_and_replays_without_scripts_or_reapplication() {
    interaction::block_on(async {
        let folder = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let original = b"# Existing rules\r\nPreserve every byte here.\r\n";
        fs::write(folder.path().join("AGENTS.md"), original).unwrap();
        fs::write(
            folder.path().join("package.json"),
            r#"{"scripts":{"build":"touch script-ran"}}"#,
        )
        .unwrap();
        let writer = scripted(0xe1, "unused-init-provider", Vec::new());
        let workspace = WorkspaceId::new([0xe2; 16]).unwrap();
        let session = SessionId::new([0xe3; 16]).unwrap();
        let service = init_service(state.path(), folder.path(), workspace, &writer);
        queue(&service, workspace).await;
        let proposal = discover(&service, workspace, 3);
        assert_eq!(fs::read(folder.path().join("AGENTS.md")).unwrap(), original);
        let applied_bytes = proposal.patch().proposed_content().as_bytes().to_vec();
        let apply = command(workspace, 0xe4, 3, WorkbenchIntent::ApplyInitDiff(proposal));
        let receipt = service.workbench_folder_command(actor(), session, &apply).await;
        assert!(matches!(receipt, AppResponsePayload::WorkbenchReceipt(_)), "{receipt:?}");
        assert_eq!(fs::read(folder.path().join("AGENTS.md")).unwrap(), applied_bytes);
        assert!(applied_bytes.starts_with(original));
        assert!(!folder.path().join(".git").exists());
        assert!(!folder.path().join("script-ran").exists());
        assert!(writer.requests.lock().unwrap().is_empty());
        fs::write(folder.path().join("AGENTS.md"), b"later independent edit\n").unwrap();
        assert_eq!(service.workbench_folder_command(actor(), session, &apply).await, receipt);
        assert_eq!(fs::read(folder.path().join("AGENTS.md")).unwrap(), b"later independent edit\n");
        service.shutdown(Duration::from_secs(5)).await;
        drop(service);
        let reopened = init_service(state.path(), folder.path(), workspace, &writer);
        reopened.with_controls(true, |_| Ok(())).unwrap();
        assert_eq!(reopened.workbench_receipt(actor(), &apply), receipt);
        assert_eq!(reopened.workbench_folder_command(actor(), session, &apply).await, receipt);
        assert_eq!(fs::read(folder.path().join("AGENTS.md")).unwrap(), b"later independent edit\n");
        reopened.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn initialization_rechecks_effective_write_policy_before_committing_authority() {
    interaction::block_on(async {
        let folder = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        let writer = scripted(0xf1, "unused-init-provider", Vec::new());
        let workspace = WorkspaceId::new([0xf2; 16]).unwrap();
        let service = init_service(state.path(), folder.path(), workspace, &writer);
        queue(&service, workspace).await;
        let restrict = command(
            workspace,
            0xf3,
            3,
            WorkbenchIntent::SetPermissions(WorkbenchPermissionChange::new(
                0,
                WorkbenchPermissionCapability::Write,
                false,
            )),
        );
        assert!(matches!(
            service.workbench_command(actor(), &restrict).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let proposal = discover(&service, workspace, 4);
        let apply = command(workspace, 0xf4, 4, WorkbenchIntent::ApplyInitDiff(proposal));
        let result = service
            .workbench_folder_command(actor(), SessionId::new([0xf5; 16]).unwrap(), &apply)
            .await;
        assert!(
            matches!(result, AppResponsePayload::Error(ref error) if error.code() == peritus_app_protocol::AppErrorCode::ReadOnly),
            "{result:?}"
        );
        assert!(!folder.path().join("AGENTS.md").exists());
        assert!(writer.requests.lock().unwrap().is_empty());
        service.shutdown(Duration::from_secs(5)).await;
    });
}

#[test]
fn initialization_discovery_rechecks_read_permission_before_inspecting_the_folder() {
    interaction::block_on(async {
        let folder = tempfile::tempdir().unwrap();
        let state = tempfile::tempdir().unwrap();
        fs::write(folder.path().join("AGENTS.md"), "PRIVATE_FOLDER_BYTES\n").unwrap();
        let writer = scripted(0xd1, "unused-init-provider", Vec::new());
        let workspace = WorkspaceId::new([0xd2; 16]).unwrap();
        let service = init_service(state.path(), folder.path(), workspace, &writer);
        queue(&service, workspace).await;
        let restrict = command(
            workspace,
            0xd3,
            3,
            WorkbenchIntent::SetPermissions(WorkbenchPermissionChange::new(
                0,
                WorkbenchPermissionCapability::Read,
                false,
            )),
        );
        assert!(matches!(
            service.workbench_command(actor(), &restrict).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));

        let response =
            service.discover_init(actor(), InitDiscoveryRequest::new(query(workspace), 4).unwrap());
        assert!(
            matches!(response, AppResponsePayload::Error(ref error) if error.code() == peritus_app_protocol::AppErrorCode::ReadOnly),
            "{response:?}"
        );
        assert_eq!(
            fs::read_to_string(folder.path().join("AGENTS.md")).unwrap(),
            "PRIVATE_FOLDER_BYTES\n"
        );
        assert!(writer.requests.lock().unwrap().is_empty());
        service.shutdown(Duration::from_secs(5)).await;
    });
}

//! Combined workbench negotiation and ordinary direct-folder IPC qualification.

use super::*;
use peritus_app_protocol::{
    InitCommandVerification, InitDiscoveryRequest, InitSourceKind, WorkbenchCheckpointName,
    WorkbenchCheckpointReceipt, WorkbenchPermissionCapability, WorkbenchPermissionChange,
    WorkbenchReceipt, WorkbenchRestoreStatus, WorkbenchRewindMode, WorkbenchRewindPreview,
    WorkbenchRewindRequest,
};

const ALL_WORKBENCH_FEATURES: [WellKnownProtocolFeature; 18] = [
    WellKnownProtocolFeature::WorkbenchControl,
    WellKnownProtocolFeature::WorkbenchInputs,
    WellKnownProtocolFeature::WorkbenchExecution,
    WellKnownProtocolFeature::WorkbenchContext,
    WellKnownProtocolFeature::WorkbenchCompaction,
    WellKnownProtocolFeature::WorkbenchBrief,
    WellKnownProtocolFeature::WorkbenchImages,
    WellKnownProtocolFeature::WorkbenchFiles,
    WellKnownProtocolFeature::WorkbenchGoals,
    WellKnownProtocolFeature::WorkbenchBudgets,
    WellKnownProtocolFeature::WorkbenchReview,
    WellKnownProtocolFeature::WorkbenchPreview,
    WellKnownProtocolFeature::WorkbenchCheckpoints,
    WellKnownProtocolFeature::ConversationLibrary,
    WellKnownProtocolFeature::ConversationForks,
    WellKnownProtocolFeature::WorkbenchPermissions,
    WellKnownProtocolFeature::WorkbenchInit,
    WellKnownProtocolFeature::WorkbenchMemory,
];

fn writable_configuration(daemon_root: &std::path::Path, folder: &std::path::Path) -> DaemonConfig {
    use std::fmt::Write as _;

    let identity = peritus_workspace::FolderIdentity::observe(folder).expect("folder identity");
    let digest = identity.digest().as_bytes().iter().fold(String::new(), |mut text, byte| {
        write!(text, "{byte:02x}").expect("hex");
        text
    });
    let text = format!(
        "{}\n[[folders]]\nworkspace_id = \"a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1\"\nroot = {:?}\nidentity = {:?}\nwritable = true\nprotected_paths = []\n",
        support::configuration_text(daemon_root),
        identity.root().to_string_lossy(),
        digest
    );
    DaemonConfig::parse(&text).expect("configured writable ordinary folder")
}

fn feature_names() -> Vec<ProtocolFeatureName> {
    ALL_WORKBENCH_FEATURES
        .into_iter()
        .map(ProtocolFeatureName::well_known)
        .collect::<Result<_, _>>()
        .expect("well-known workbench features")
}

async fn connect_all(
    endpoint: LocalEndpointAddress,
    requested_session: Option<peritus_types::SessionId>,
) -> (AppFrameStream<UnixStream>, ProtocolContext) {
    let LocalEndpointAddress::Unix(socket) = endpoint;
    let mut frames = AppFrameStream::new(
        UnixStream::connect(socket).await.expect("connect"),
        AppProtocolLimits::PRODUCTION,
    );
    let hello = ClientHello::new_with_session(
        ProtocolId::new([0xa2; 16]).expect("protocol"),
        requested_session,
        vec![VersionRange::new(1, 0, 0).expect("version")],
        feature_names(),
        Vec::new(),
        AppProtocolLimits::PRODUCTION,
        "integrated-workbench-test".to_owned(),
    )
    .expect("hello");
    frames.write(&AppMessage::ClientHello(hello.clone())).await.expect("hello write");
    let AppMessage::ServerHello(server) = frames.read().await.expect("server hello") else {
        panic!("server hello")
    };
    let negotiated = match server.outcome() {
        NegotiationOutcome::Compatible(value) | NegotiationOutcome::Downgraded(value) => value,
        NegotiationOutcome::Incompatible(reason) => panic!("incompatible: {reason:?}"),
    };
    assert_eq!(negotiated.features(), hello.required_features());
    let session = server.established_session().expect("established session");
    assert!(requested_session.is_none_or(|requested| requested == session));
    let context = ProtocolContext::new(hello.protocol_id(), negotiated.version(), session);
    (frames, context)
}

fn workbench_query() -> WorkbenchQuery {
    WorkbenchQuery::new(
        ConversationId::new([0xa3; 16]).expect("conversation"),
        peritus_types::WorkspaceId::new([0xa1; 16]).expect("workspace"),
    )
}

fn workbench_command(id: u8, revision: u64, intent: WorkbenchIntent) -> WorkbenchCommand {
    WorkbenchCommand::new(
        ControlOperationId::new([id; 16]).expect("operation"),
        workbench_query(),
        revision,
        intent,
    )
}

async fn create_conversation(
    client: &mut (AppFrameStream<UnixStream>, ProtocolContext),
) -> WorkbenchReceipt {
    let command = workbench_command(
        0xa4,
        0,
        WorkbenchIntent::CreateConversation(
            ConversationTitle::new("Integrated ordinary folder".to_owned()).expect("title"),
        ),
    );
    let response = request(client, 1, AppRequestPayload::WorkbenchCommand(command.clone())).await;
    let AppResponsePayload::WorkbenchReceipt(receipt) = response else {
        panic!("create receipt: {response:?}")
    };
    assert_eq!(receipt.operation(), command.operation());
    assert_eq!(receipt.query(), workbench_query());
    assert_eq!(receipt.accepted_revision(), 1);
    receipt
}

async fn discover_init(
    client: &mut (AppFrameStream<UnixStream>, ProtocolContext),
) -> peritus_app_protocol::InitProposal {
    let discovery =
        InitDiscoveryRequest::new(workbench_query(), 1).expect("init discovery request");
    let response = request(client, 2, AppRequestPayload::DiscoverInit(discovery)).await;
    let AppResponsePayload::InitProposal(proposal) = response else {
        panic!("init proposal: {response:?}")
    };
    assert_eq!(proposal.query(), workbench_query());
    assert_eq!(proposal.revision(), 1);
    assert!(proposal.sources().iter().any(|source| {
        source.path() == "AGENTS.md" && source.kind() == InitSourceKind::Instructions
    }));
    assert!(
        proposal
            .commands()
            .iter()
            .all(|command| { command.verification() == InitCommandVerification::Unverified })
    );
    proposal
}

async fn apply_init(
    client: &mut (AppFrameStream<UnixStream>, ProtocolContext),
    proposal: peritus_app_protocol::InitProposal,
) -> (WorkbenchCommand, AppResponsePayload) {
    let command = workbench_command(0xa5, 1, WorkbenchIntent::ApplyInitDiff(proposal));
    let response = request(client, 3, AppRequestPayload::WorkbenchCommand(command.clone())).await;
    assert!(
        matches!(&response, AppResponsePayload::WorkbenchReceipt(receipt)
            if receipt.operation() == command.operation()
                && receipt.query() == workbench_query()
                && receipt.accepted_revision() == 2),
        "init receipt: {response:?}"
    );
    (command, response)
}

async fn create_and_inspect_checkpoint(
    client: &mut (AppFrameStream<UnixStream>, ProtocolContext),
) -> WorkbenchCheckpointReceipt {
    let command = workbench_command(
        0xa6,
        2,
        WorkbenchIntent::CreateCheckpoint(
            WorkbenchCheckpointName::new("After initialization".to_owned()).expect("name"),
        ),
    );
    let response = request(client, 4, AppRequestPayload::WorkbenchCommand(command.clone())).await;
    let AppResponsePayload::WorkbenchCheckpoint(created) = response else {
        panic!("checkpoint receipt: {response:?}")
    };
    assert_eq!(created.checkpoint(), command.operation());
    assert_eq!(created.accepted_revision(), 3);
    assert_eq!(created.references().source_conversation_revision(), 2);

    let inspection = WorkbenchRewindRequest::new(workbench_query(), 3, command.operation())
        .expect("checkpoint inspection");
    let response =
        request(client, 5, AppRequestPayload::InspectWorkbenchCheckpoint(inspection)).await;
    let AppResponsePayload::WorkbenchCheckpoint(inspected) = response else {
        panic!("inspected checkpoint: {response:?}")
    };
    assert_eq!(inspected, created);
    created
}

async fn preview_rewind(
    client: &mut (AppFrameStream<UnixStream>, ProtocolContext),
    sequence: u8,
    revision: u64,
    checkpoint: ControlOperationId,
    mode: WorkbenchRewindMode,
    child: u8,
) -> WorkbenchRewindPreview {
    let mut rewind = WorkbenchRewindRequest::new(workbench_query(), revision, checkpoint)
        .expect("rewind request");
    if mode != WorkbenchRewindMode::FilesOnly {
        rewind = rewind
            .with_branch(mode, ConversationId::new([child; 16]).expect("rewind child"), None)
            .expect("logical rewind branch");
    }
    let response =
        request(client, sequence, AppRequestPayload::PreviewWorkbenchRewind(rewind)).await;
    let AppResponsePayload::WorkbenchRewindPreview(preview) = response else {
        panic!("rewind preview: {response:?}")
    };
    preview
}

#[test]
fn all_workbench_features_init_and_checkpoint_round_trip_over_authenticated_ipc() {
    run_async_test(async {
        let temporary = support::temporary_root();
        let folder = temporary.path().join("ordinary-folder");
        std::fs::create_dir(&folder).expect("folder");
        let original = b"# Existing rules\r\nPreserve these exact bytes.\r\n";
        std::fs::write(folder.join("AGENTS.md"), original).expect("instructions");
        std::fs::write(folder.join("package.json"), br#"{"scripts":{"build":"touch script-ran"}}"#)
            .expect("manifest");
        let config = writable_configuration(temporary.path(), &folder);
        let runtime = DaemonRuntime::start(config).await.expect("start");
        let endpoint = runtime.endpoint_address().clone();
        let mut client = connect_all(endpoint.clone(), None).await;

        create_conversation(&mut client).await;
        let proposal = discover_init(&mut client).await;
        assert_eq!(std::fs::read(folder.join("AGENTS.md")).expect("read"), original);
        let applied = proposal.patch().proposed_content().as_bytes().to_vec();
        assert!(applied.starts_with(original));
        let (init, original_receipt) = apply_init(&mut client, proposal).await;
        assert_eq!(std::fs::read(folder.join("AGENTS.md")).expect("applied"), applied);
        assert!(!folder.join("script-ran").exists());
        assert!(!folder.join(".git").exists());
        let checkpoint = create_and_inspect_checkpoint(&mut client).await;

        restrict_write(&mut client).await;

        let logical = preview_rewind(
            &mut client,
            7,
            4,
            checkpoint.checkpoint(),
            WorkbenchRewindMode::ConversationOnly,
            0xa8,
        )
        .await;
        assert!(logical.paths().is_empty());
        let logical_apply = workbench_command(0xa9, 4, WorkbenchIntent::ApplyRewind(logical));
        let response =
            request(&mut client, 8, AppRequestPayload::WorkbenchCommand(logical_apply)).await;
        let AppResponsePayload::WorkbenchRestore(logical_restore) = response else {
            panic!("conversation-only rewind: {response:?}")
        };
        assert_eq!(logical_restore.status(), WorkbenchRestoreStatus::Applied);
        assert!(logical_restore.restored().is_empty());
        assert_eq!(std::fs::read(folder.join("AGENTS.md")).expect("logical rewind bytes"), applied);

        let revision = logical_restore.accepted_revision();
        for (preview_sequence, apply_sequence, operation, mode, child) in [
            (9, 10, 0xaa, WorkbenchRewindMode::FilesOnly, 0),
            (11, 12, 0xab, WorkbenchRewindMode::Combined, 0xac),
        ] {
            let preview = preview_rewind(
                &mut client,
                preview_sequence,
                revision,
                checkpoint.checkpoint(),
                mode,
                child,
            )
            .await;
            let denied =
                workbench_command(operation, revision, WorkbenchIntent::ApplyRewind(preview));
            assert_error(
                request(&mut client, apply_sequence, AppRequestPayload::WorkbenchCommand(denied))
                    .await,
                AppErrorCode::ReadOnly,
            );
        }
        let snapshot =
            request(&mut client, 13, AppRequestPayload::QueryWorkbench(workbench_query())).await;
        assert!(
            matches!(snapshot, AppResponsePayload::Workbench(ref value)
                if value.revision() == revision),
            "denied rewinds changed the conversation: {snapshot:?}"
        );
        assert_eq!(std::fs::read(folder.join("AGENTS.md")).expect("denied rewind bytes"), applied);

        let session = client.1.session_id();
        drop(client);
        std::fs::write(folder.join("AGENTS.md"), b"later independent edit\n").expect("edit");
        let mut resumed = connect_all(endpoint, Some(session)).await;
        assert_eq!(
            request(&mut resumed, 14, AppRequestPayload::QueryWorkbenchReceipt(init.clone()),)
                .await,
            original_receipt,
        );
        assert_error(
            request(&mut resumed, 15, AppRequestPayload::WorkbenchCommand(init)).await,
            AppErrorCode::ReadOnly,
        );
        assert_eq!(
            std::fs::read(folder.join("AGENTS.md")).expect("replayed state"),
            b"later independent edit\n"
        );
        assert!(!folder.join("script-ran").exists());
        drop(resumed);
        runtime.shutdown().await.expect("shutdown");
    });
}

async fn restrict_write(client: &mut (AppFrameStream<UnixStream>, ProtocolContext)) {
    let command = workbench_command(
        0xa7,
        3,
        WorkbenchIntent::SetPermissions(WorkbenchPermissionChange::new(
            0,
            WorkbenchPermissionCapability::Write,
            false,
        )),
    );
    let restricted = request(client, 6, AppRequestPayload::WorkbenchCommand(command)).await;
    assert!(
        matches!(restricted, AppResponsePayload::WorkbenchReceipt(ref receipt)
            if receipt.accepted_revision() == 4),
        "permission receipt: {restricted:?}"
    );
}

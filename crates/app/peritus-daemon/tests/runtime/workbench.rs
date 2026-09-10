//! Real authenticated workbench IPC, reconnect and restart receipt recovery.

use super::*;
use peritus_app_protocol::{
    AppErrorCode, ControlOperationId, ConversationId, ConversationTitle, ProtocolFeatureName,
    WellKnownProtocolFeature, WorkbenchCommand, WorkbenchIntent, WorkbenchQuery,
};

#[path = "workbench/brief.rs"]
mod brief;
#[path = "workbench/context.rs"]
mod context_inspection;
#[path = "workbench/files.rs"]
mod files;
#[path = "workbench/images.rs"]
mod images;
#[path = "workbench/integrated.rs"]
mod integrated;
#[path = "workbench/queue.rs"]
mod queue;

fn configuration(root: &std::path::Path) -> DaemonConfig {
    DaemonConfig::parse(&configuration_text(root)).expect("configured read-only folder")
}

fn configuration_text(root: &std::path::Path) -> String {
    use std::fmt::Write as _;
    let identity = peritus_workspace::FolderIdentity::observe(root).expect("folder identity");
    let digest = identity.digest().as_bytes().iter().fold(String::new(), |mut text, byte| {
        write!(text, "{byte:02x}").expect("hex");
        text
    });
    format!(
        "{}\n[[folders]]\nworkspace_id = \"33333333333333333333333333333333\"\nroot = {:?}\nidentity = {:?}\nwritable = false\nprotected_paths = []\n",
        support::configuration_text(root),
        identity.root().to_string_lossy(),
        digest
    )
}

async fn connect(
    endpoint: LocalEndpointAddress,
    enabled: bool,
) -> (AppFrameStream<UnixStream>, ProtocolContext) {
    let LocalEndpointAddress::Unix(socket) = endpoint;
    let mut frames = AppFrameStream::new(
        UnixStream::connect(socket).await.expect("connect"),
        AppProtocolLimits::PRODUCTION,
    );
    let features = if enabled {
        vec![
            ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchControl)
                .expect("feature"),
            ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchInputs)
                .expect("inputs feature"),
            ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchContext)
                .expect("context feature"),
            ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchBrief)
                .expect("brief feature"),
            ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchImages)
                .expect("images feature"),
            ProtocolFeatureName::well_known(WellKnownProtocolFeature::WorkbenchFiles)
                .expect("files feature"),
        ]
    } else {
        Vec::new()
    };
    let hello = ClientHello::new(
        ProtocolId::new([43; 16]).expect("protocol"),
        vec![VersionRange::new(1, 0, 0).expect("version")],
        Vec::new(),
        features,
        AppProtocolLimits::PRODUCTION,
        "workbench-test".to_owned(),
    )
    .expect("hello");
    frames.write(&AppMessage::ClientHello(hello.clone())).await.expect("hello");
    let AppMessage::ServerHello(server) = frames.read().await.expect("server") else {
        panic!("server hello")
    };
    let protocol = match server.outcome() {
        NegotiationOutcome::Compatible(value) | NegotiationOutcome::Downgraded(value) => value,
        NegotiationOutcome::Incompatible(reason) => panic!("incompatible: {reason:?}"),
    };
    let context = ProtocolContext::new(
        hello.protocol_id(),
        protocol.version(),
        server.established_session().expect("session"),
    );
    (frames, context)
}

async fn request(
    client: &mut (AppFrameStream<UnixStream>, ProtocolContext),
    sequence: u8,
    payload: AppRequestPayload,
) -> AppResponsePayload {
    let envelope = AppRequestEnvelope::new(
        client.1,
        RequestId::new([sequence; 16]).expect("request"),
        CorrelationId::new([sequence; 16]).expect("correlation"),
        payload,
    )
    .expect("envelope");
    client.0.write(&AppMessage::Request(envelope.clone())).await.expect("write");
    let AppMessage::Response(response) = client.0.read().await.expect("response") else {
        panic!("response")
    };
    assert_eq!(response.request_id(), envelope.request_id());
    response.payload().clone()
}

fn assert_error(payload: AppResponsePayload, code: AppErrorCode) {
    let AppResponsePayload::Error(error) = payload else {
        panic!("expected {code:?}, got {payload:?}")
    };
    assert_eq!(error.code(), code);
}

#[test]
fn control_negotiation_exact_receipts_and_stale_edits_survive_daemon_restart() {
    run_async_test(async {
        let temporary = support::temporary_root();
        let config = configuration(temporary.path());
        let runtime = DaemonRuntime::start(config.clone()).await.expect("start");
        let query = WorkbenchQuery::new(
            ConversationId::new([44; 16]).expect("conversation"),
            peritus_types::WorkspaceId::new([0x33; 16]).expect("workspace"),
        );
        let command = WorkbenchCommand::new(
            ControlOperationId::new([45; 16]).expect("operation"),
            query,
            0,
            WorkbenchIntent::CreateConversation(
                ConversationTitle::new("Original title".to_owned()).expect("title"),
            ),
        );
        let mut denied = connect(runtime.endpoint_address().clone(), false).await;
        assert_error(
            request(&mut denied, 1, AppRequestPayload::WorkbenchCommand(command.clone())).await,
            AppErrorCode::MissingRequiredFeature,
        );
        drop(denied);
        let mut client = connect(runtime.endpoint_address().clone(), true).await;
        assert_error(
            request(&mut client, 1, AppRequestPayload::QueryWorkbench(query)).await,
            AppErrorCode::InvalidIdentifier,
        );
        assert!(
            !temporary.path().join("state/workbench-v1").exists(),
            "inspection must not initialize a journal"
        );
        let original =
            request(&mut client, 2, AppRequestPayload::WorkbenchCommand(command.clone())).await;
        let AppResponsePayload::WorkbenchReceipt(receipt) = &original else {
            panic!("receipt: {original:?}")
        };
        assert_eq!(receipt.accepted_revision(), 1);
        let rename = WorkbenchCommand::new(
            ControlOperationId::new([46; 16]).expect("operation"),
            query,
            1,
            WorkbenchIntent::RenameConversation(
                ConversationTitle::new("New title".to_owned()).expect("title"),
            ),
        );
        assert!(matches!(
            request(&mut client, 3, AppRequestPayload::WorkbenchCommand(rename.clone())).await,
            AppResponsePayload::WorkbenchReceipt(_)
        ));
        let stale = WorkbenchCommand::new(
            ControlOperationId::new([47; 16]).expect("operation"),
            query,
            1,
            WorkbenchIntent::PinConversation(true),
        );
        assert_error(
            request(&mut client, 4, AppRequestPayload::WorkbenchCommand(stale)).await,
            AppErrorCode::StaleRevision,
        );
        let conflict = WorkbenchCommand::new(
            command.operation(),
            query,
            2,
            WorkbenchIntent::PinConversation(true),
        );
        assert_error(
            request(&mut client, 5, AppRequestPayload::WorkbenchCommand(conflict)).await,
            AppErrorCode::IdempotencyConflict,
        );
        drop(client);
        runtime.shutdown().await.expect("shutdown");
        let restarted = DaemonRuntime::start(config).await.expect("restart");
        let mut client = connect(restarted.endpoint_address().clone(), true).await;
        assert_eq!(
            request(&mut client, 1, AppRequestPayload::QueryWorkbenchReceipt(command.clone()))
                .await,
            original
        );
        assert_eq!(
            request(&mut client, 2, AppRequestPayload::WorkbenchCommand(command)).await,
            original
        );
        let snapshot = request(&mut client, 3, AppRequestPayload::QueryWorkbench(query)).await;
        let AppResponsePayload::Workbench(snapshot) = snapshot else { panic!("snapshot") };
        assert_eq!(snapshot.revision(), 2);
        assert_eq!(snapshot.title().as_str(), "New title");
        assert!(!snapshot.pinned());
        let doctor = request(
            &mut client,
            4,
            AppRequestPayload::QueryProductRuns(peritus_app_protocol::ProductRunQuery::recent()),
        )
        .await;
        assert!(
            matches!(doctor, AppResponsePayload::ProductRunSettlements(ref runs) if runs.is_empty())
        );
        drop(client);
        restarted.shutdown().await.expect("shutdown");
    });
}

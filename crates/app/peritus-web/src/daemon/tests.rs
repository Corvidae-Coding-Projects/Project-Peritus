//! Native conversation wire contract against a local protocol fixture; no provider calls.

mod recovery;
mod sessions;

use super::*;
use crate::config::Options;
use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppResponseEnvelope, ProductActivity, ProductActivityKind,
    ProductInteractionSnapshot, ProductRunControlAction, ProductRunLegalControls,
    ProductRunOperation, ProductRunOperationKind, ProductRunOperationState, ProductRunPhase,
    ServerCapabilities, WorkbenchCommand, WorkbenchExecutionState, WorkbenchIntent,
    WorkbenchReceipt, WorkbenchSnapshot, decode_app_message, encode_app_message, negotiate,
};
use peritus_codec::HEADER_LEN;
use peritus_types::SessionId;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
};

async fn read_message(stream: &mut UnixStream) -> AppMessage {
    let mut header = [0; HEADER_LEN];
    stream.read_exact(&mut header).await.unwrap();
    let length = u32::from_be_bytes(header[12..16].try_into().unwrap()) as usize;
    assert!(length <= AppProtocolLimits::PRODUCTION.codec().max_payload_bytes);
    let mut bytes = header.to_vec();
    bytes.resize(HEADER_LEN + length, 0);
    stream.read_exact(&mut bytes[HEADER_LEN..]).await.unwrap();
    decode_app_message(&bytes, AppProtocolLimits::PRODUCTION).unwrap()
}

async fn write_message(stream: &mut UnixStream, message: AppMessage) {
    let bytes = encode_app_message(&message, AppProtocolLimits::PRODUCTION).unwrap();
    stream.write_all(&bytes).await.unwrap();
}

fn operation(run: RunId, state: ProductRunOperationState) -> ProductRunOperation {
    let controls = match state {
        ProductRunOperationState::Running | ProductRunOperationState::WaitingForUser => {
            ProductRunLegalControls::none().with(ProductRunControlAction::Cancel)
        }
        ProductRunOperationState::Failed
        | ProductRunOperationState::Cancelled
        | ProductRunOperationState::RecoveryRequired => {
            ProductRunLegalControls::none().with(ProductRunControlAction::Retry)
        }
        ProductRunOperationState::Succeeded | ProductRunOperationState::OutcomeUnknown => {
            ProductRunLegalControls::none()
        }
    };
    ProductRunOperation::new(
        ProductRunOperationKind::Execution,
        state,
        format!("run/{}", hex(run.as_bytes())),
        "The native fixture owns this exact execution observation.".to_owned(),
        String::new(),
        controls,
    )
    .expect("operation")
}

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "keeps the exact socket sequence and its durable identities in one contract"
)]
async fn native_message_uses_durable_conversation_queue_and_execution_receipts() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path().canonicalize().unwrap();
    let config = root.join("native-config");
    let product_state = root.join("native-state/product-state");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::create_dir_all(&product_state).unwrap();
    let writer = ProviderProfileId::new([2; 16]).unwrap();
    let reviewer = ProviderProfileId::new([3; 16]).unwrap();
    let workspace = WorkspaceId::new([4; 16]).unwrap();
    std::fs::write(
        config.join("peritus-00000000000000000001.toml"),
        format!(
            "[[providers]]\nkind = 'fixture'\n[providers.profile]\nprofile_id = '{}'\nmodel = 'fixture-default'\n",
            hex(writer.as_bytes()),
        ),
    ).unwrap();
    std::fs::write(
        product_state.join("state-00000000000000000001.json"),
        serde_json::to_vec(&json!({"workspaces":{"recent":[],"retained_registrations":[{
            "workspace_id":hex(workspace.as_bytes()), "repository_root":root,
            "managed_root":root.join("execution"), "trust":"trusted"
        }]}}))
        .unwrap(),
    )
    .unwrap();
    let endpoint = root.join("fixture.sock");
    let listener = UnixListener::bind(&endpoint).unwrap();
    let app = App::open(
        Options {
            port: 4173,
            root: root.clone(),
            assets: root.join("assets"),
            config_file: root.join("presentation/preferences.toml"),
            state_file: root.join("presentation/workspace.json"),
            daemon_config_root: config,
            product_state_root: product_state,
            daemon_config: None,
            endpoint: Some(endpoint),
            cli: "peritus".into(),
        },
        4173,
    )
    .unwrap();
    let session = app.snapshot().unwrap().sessions[0].clone();
    std::fs::write(root.join("attached.txt"), "Immutable attachment snapshot λ界\n".repeat(12_000))
        .unwrap();
    let attachment=crate::files::attachments::stage(&app,&json!({"session":session.id,"project":app.snapshot().unwrap().projects[0].id,"path":"attached.txt"})).unwrap();
    std::fs::write(root.join("attached.txt"), "Later disk edits must not leak into this message")
        .unwrap();
    let expected_run = RunId::new(bytes(&session.run).unwrap()).unwrap();
    let expected_query = WorkbenchQuery::new(
        ConversationId::new(bytes(&session.conversation).unwrap()).unwrap(),
        workspace,
    );
    let expected_providers = ProductProviderSelection::new(writer, reviewer, writer);
    let expected_models = ProductRoleModels::new(
        ProductModelChoice::new("fixture-writer".into(), true)
            .unwrap()
            .with_effort(ProductModelEffort::High),
        ProductModelChoice::new("fixture-reviewer".into(), false)
            .unwrap()
            .with_effort(ProductModelEffort::Medium),
        ProductModelChoice::default(),
    );
    let server = tokio::spawn(async move {
        let mut execution_queries = 0;
        let mut imported = Vec::new();
        loop {
            let (mut stream, envelope) = receive_request(&listener).await;
            let payload = match envelope.payload() {
                AppRequestPayload::BeginWorkbenchFileUpload(_) => {
                    imported = receive_snapshot_batch(&mut stream, envelope, 2).await;
                    assert!(imported[0].1.starts_with(b"Fixture message"));
                    assert!(imported[0].1.len() > 300_000);
                    assert!(imported[1].1.starts_with(b"Immutable attachment snapshot"));
                    assert!(imported[1].1.len() > 300_000);
                    continue;
                }
                AppRequestPayload::DaemonStatus => AppResponsePayload::DaemonStatus(
                    peritus_app_protocol::DaemonStatus::new(
                        peritus_app_protocol::DaemonReadiness::ReadyReadWrite,
                        None,
                        1024,
                    )
                    .unwrap(),
                ),
                AppRequestPayload::Doctor(query) => AppResponsePayload::Doctor(
                    peritus_app_protocol::DoctorReport::new(
                        *query,
                        vec![
                            peritus_app_protocol::DoctorFinding::new(
                                "workspace-policy".into(),
                                peritus_app_protocol::DoctorStatus::Healthy,
                                "Fixture admitted workspace".into(),
                                String::new(),
                            )
                            .unwrap(),
                            peritus_app_protocol::DoctorFinding::new(
                                "provider-route".into(),
                                peritus_app_protocol::DoctorStatus::Healthy,
                                "Fixture provider".into(),
                                String::new(),
                            )
                            .unwrap(),
                        ],
                    )
                    .unwrap(),
                ),
                AppRequestPayload::QueryWorkbenchExecution(query) => {
                    assert_eq!(*query, expected_query);
                    execution_queries += 1;
                    if execution_queries == 1 {
                        AppResponsePayload::Error(peritus_app_protocol::AppProtocolError::new(
                            AppErrorCode::InvalidIdentifier,
                            None,
                        ))
                    } else {
                        AppResponsePayload::WorkbenchExecution(
                            WorkbenchExecutionState::new(
                                WorkbenchSnapshot::new(
                                    expected_query,
                                    2,
                                    ConversationTitle::new("New conversation".into()).unwrap(),
                                    false,
                                    false,
                                )
                                .unwrap(),
                                None,
                                false,
                            )
                            .unwrap(),
                        )
                    }
                }
                AppRequestPayload::WorkbenchCommand(command) => {
                    assert_eq!(command.query(), expected_query);
                    let revision = match command.intent() {
                        WorkbenchIntent::CreateConversation(title) => {
                            assert_eq!(title.as_str(), "New conversation");
                            assert_eq!(command.expected_revision(), 0);
                            1
                        }
                        WorkbenchIntent::EnqueueMessageBundle { text, message, attachments } => {
                            assert_eq!(command.expected_revision(), 1);
                            assert!(text.as_str().contains("user_message"));
                            assert_eq!(message.as_ref(), Some(&imported[0].0));
                            assert_eq!(attachments, &vec![imported[1].0.clone()]);
                            2
                        }
                        WorkbenchIntent::StartExecution(settings) => {
                            assert_eq!(command.expected_revision(), 2);
                            assert_eq!(settings.run(), expected_run);
                            assert_eq!(settings.providers(), expected_providers);
                            assert_eq!(settings.mode(), ProductInteractionMode::Build);
                            assert_eq!(settings.models(), &expected_models);
                            3
                        }
                        intent => panic!("unexpected workbench intent: {intent:?}"),
                    };
                    AppResponsePayload::WorkbenchReceipt(
                        WorkbenchReceipt::new(
                            command.operation(),
                            expected_query,
                            revision,
                            peritus_types::Sha256Digest::new([u8::try_from(revision).unwrap(); 32]),
                        )
                        .unwrap(),
                    )
                }
                AppRequestPayload::QueryInteraction(query) => {
                    assert_eq!(query.run_id(), expected_run);
                    let snapshot = ProductRunSnapshot::new(
                        expected_run,
                        workspace,
                        expected_providers,
                        ProductRunPhase::Queued,
                        0,
                        "Fixture task".into(),
                        "Queued by fixture".into(),
                        String::new(),
                        String::new(),
                        String::new(),
                        String::new(),
                        operation(expected_run, ProductRunOperationState::Running),
                    )
                    .unwrap();
                    let observation = ProductInteractionSnapshot::new(
                        snapshot,
                        ProductInteractionMode::Build,
                        expected_models.clone(),
                        1,
                        0,
                        vec![
                            ProductActivity::new(
                                7,
                                ProductActivityKind::Status,
                                "Fixture receipt".into(),
                                "Not yet incorporated".into(),
                            )
                            .unwrap(),
                        ],
                        None,
                    )
                    .unwrap();
                    let payload = AppResponsePayload::Interaction(observation);
                    write_message(
                        &mut stream,
                        AppMessage::Response(AppResponseEnvelope::new(
                            envelope.context(),
                            envelope.request_id(),
                            envelope.correlation_id(),
                            payload,
                        )),
                    )
                    .await;
                    break;
                }
                payload => panic!("unexpected request: {payload:?}"),
            };
            write_message(
                &mut stream,
                AppMessage::Response(AppResponseEnvelope::new(
                    envelope.context(),
                    envelope.request_id(),
                    envelope.correlation_id(),
                    payload,
                )),
            )
            .await;
        }
    });
    let operation = crate::state::id().unwrap();
    let input = json!({
        "operation":operation,"session":session.id, "text":format!("Fixture message — unchanged {}", "λ界 exact instructions\n".repeat(20_000)), "mode":"build",
        "attachments":[attachment["id"].clone()],
        "providers":{"writer":"","reviewer":hex(reviewer.as_bytes()),"fixer":""},
        "models":{
            "writer":{"id":"fixture-writer","manual":true,"effort":"high"},
            "reviewer":{"id":"fixture-reviewer","manual":false,"effort":"medium"}
        }
    });
    app.record_operation(operation, input.clone()).unwrap();
    let output =
        tokio::time::timeout(Duration::from_secs(3), send(&app, &input)).await.unwrap().unwrap();
    assert_eq!(output["run"]["id"], session.run);
    assert_eq!(output["run"]["workspace"], hex(workspace.as_bytes()));
    assert_eq!(output["run"]["operation"]["state"], "Running");
    assert_eq!(output["received"], "1");
    assert_eq!(output["incorporated"], "0");
    assert_eq!(
        output["activities"][0],
        json!({"id":"7","kind":"status",
        "text":"Fixture receipt","detail":"Not yet incorporated"})
    );
    tokio::time::timeout(Duration::from_secs(3), server).await.unwrap().unwrap();
}

fn isolated_app(root: &Path, endpoint: PathBuf) -> App {
    App::open(
        Options {
            port: 4173,
            root: root.into(),
            assets: root.join("assets"),
            config_file: root.join("webui.toml"),
            state_file: root.join("workspace.json"),
            daemon_config_root: root.into(),
            product_state_root: root.into(),
            daemon_config: None,
            endpoint: Some(endpoint),
            cli: "peritus".into(),
        },
        4173,
    )
    .unwrap()
}
async fn receive_request(
    listener: &UnixListener,
) -> (UnixStream, peritus_app_protocol::AppRequestEnvelope) {
    let (mut stream, _) = listener.accept().await.unwrap();
    let AppMessage::ClientHello(hello) = read_message(&mut stream).await else { panic!("hello") };
    let capabilities = ServerCapabilities::new(
        vec![peritus_app_protocol::CURRENT_PROTOCOL_RANGE],
        hello.required_features().as_slice().to_vec(),
        AppProtocolLimits::PRODUCTION,
        "recovery fixture".into(),
    )
    .unwrap();
    let answer = negotiate(&hello, &capabilities, SessionId::new([9; 16]).unwrap()).unwrap();
    write_message(&mut stream, AppMessage::ServerHello(answer)).await;
    let AppMessage::Request(request) = read_message(&mut stream).await else { panic!("request") };
    (stream, request)
}
#[tokio::test]
async fn read_only_or_draining_connection_is_not_mutation_ready() {
    use peritus_app_protocol::{DaemonReadiness, DaemonStatus};
    for readiness in [DaemonReadiness::ReadyReadOnly, DaemonReadiness::Draining] {
        let temporary = tempfile::tempdir().unwrap();
        let endpoint = temporary.path().join("ready.sock");
        let listener = UnixListener::bind(&endpoint).unwrap();
        let app = isolated_app(temporary.path(), endpoint);
        let server = tokio::spawn(async move {
            let (mut stream, request) = receive_request(&listener).await;
            assert_eq!(request.payload(), &AppRequestPayload::DaemonStatus);
            write_message(
                &mut stream,
                AppMessage::Response(AppResponseEnvelope::new(
                    request.context(),
                    request.request_id(),
                    request.correlation_id(),
                    AppResponsePayload::DaemonStatus(
                        DaemonStatus::new(readiness, None, 1024).unwrap(),
                    ),
                )),
            )
            .await;
        });
        let observed = status(&app).await.unwrap();
        assert_eq!(observed["connected"], true);
        assert_eq!(observed["ready"], false);
        server.await.unwrap();
    }
}

async fn receive_snapshot_batch(
    stream: &mut UnixStream,
    mut request: peritus_app_protocol::AppRequestEnvelope,
    count: usize,
) -> Vec<(peritus_app_protocol::WorkbenchFileImportPreview, Vec<u8>)> {
    let mut snapshots = Vec::new();
    let mut bytes = Vec::new();
    loop {
        let payload = match request.payload() {
            AppRequestPayload::BeginWorkbenchFileUpload(_) => {
                bytes.clear();
                AppResponsePayload::Acknowledged(
                    peritus_app_protocol::OperationAcknowledgement::new(request.request_id()),
                )
            }
            AppRequestPayload::UploadArtifactChunk(chunk) => {
                assert_eq!(chunk.offset(), bytes.len() as u64);
                bytes.extend_from_slice(chunk.bytes());
                AppResponsePayload::Acknowledged(
                    peritus_app_protocol::OperationAcknowledgement::new(request.request_id()),
                )
            }
            AppRequestPayload::CompleteArtifactUpload(_) => AppResponsePayload::Acknowledged(
                peritus_app_protocol::OperationAcknowledgement::new(request.request_id()),
            ),
            AppRequestPayload::PreviewWorkbenchFileImport(selected) => {
                assert_eq!(selected.file().bytes(), bytes.len() as u64);
                assert_eq!(
                    selected.file().digest().as_bytes(),
                    &<[u8; 32]>::from(Sha256::digest(&bytes))
                );
                let preview = peritus_app_protocol::WorkbenchFileImportPreview::new(
                    selected.clone(),
                    1,
                    "fixture-writer".into(),
                )
                .unwrap();
                snapshots.push((preview.clone(), bytes.clone()));
                AppResponsePayload::WorkbenchFileImportPreview(preview)
            }
            other => panic!("unexpected transfer payload {other:?}"),
        };
        write_message(
            stream,
            AppMessage::Response(AppResponseEnvelope::new(
                request.context(),
                request.request_id(),
                request.correlation_id(),
                payload,
            )),
        )
        .await;
        if snapshots.len() == count {
            break;
        }
        let AppMessage::Request(next) = read_message(stream).await else {
            panic!("upload request")
        };
        request = next;
    }
    snapshots
}

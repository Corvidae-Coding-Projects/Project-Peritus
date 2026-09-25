//! Native conversation wire contract against a local protocol fixture; no provider calls.

mod sessions;

use super::*;
use crate::config::Options;
use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppResponseEnvelope, ProductActivity, ProductActivityKind,
    ProductInteractionSnapshot, ProductRunPhase, ServerCapabilities, VersionRange,
    decode_app_message, encode_app_message, negotiate,
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

#[tokio::test]
#[expect(
    clippy::too_many_lines,
    reason = "keeps all four modes, exact wire assertions, and socket fixture lifecycle in one contract test"
)]
async fn native_messages_preserve_targets_modes_models_and_observed_revisions() {
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
    let session = app.snapshot().unwrap().sessions[0].id.clone();
    std::fs::write(root.join("attached.txt"), "Immutable attachment snapshot").unwrap();
    let attachment=crate::files::attachments::stage(&app,&json!({"session":session,"project":app.snapshot().unwrap().projects[0].id,"path":"attached.txt"})).unwrap();
    std::fs::write(root.join("attached.txt"), "Later disk edits must not leak into this message")
        .unwrap();
    let expected_run = RunId::new(bytes(&session).unwrap()).unwrap();
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
    let modes = [
        ProductInteractionMode::Chat,
        ProductInteractionMode::Plan,
        ProductInteractionMode::Review,
        ProductInteractionMode::Build,
    ];
    let server = tokio::spawn(async move {
        for mode in modes {
            loop {
                let (mut stream, _) = listener.accept().await.unwrap();
                let AppMessage::ClientHello(hello) = read_message(&mut stream).await else {
                    panic!("expected client negotiation");
                };
                let capabilities = ServerCapabilities::new(
                    vec![VersionRange::new(1, 0, 0).unwrap()],
                    vec![
                        peritus_app_protocol::ProtocolFeatureName::well_known(
                            peritus_app_protocol::WellKnownProtocolFeature::ProductDiagnostics,
                        )
                        .unwrap(),
                    ],
                    AppProtocolLimits::PRODUCTION,
                    "web gateway fixture".into(),
                )
                .unwrap();
                let answer =
                    negotiate(&hello, &capabilities, SessionId::new([9; 16]).unwrap()).unwrap();
                write_message(&mut stream, AppMessage::ServerHello(answer)).await;
                let AppMessage::Request(envelope) = read_message(&mut stream).await else {
                    panic!("expected request");
                };
                let diagnostic = match envelope.payload() {
                    AppRequestPayload::DaemonStatus => Some(AppResponsePayload::DaemonStatus(
                        peritus_app_protocol::DaemonStatus::new(
                            peritus_app_protocol::DaemonReadiness::ReadyReadWrite,
                            None,
                            1024,
                        )
                        .unwrap(),
                    )),
                    AppRequestPayload::Doctor(query) => Some(AppResponsePayload::Doctor(
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
                    )),
                    _ => None,
                };
                if let Some(payload) = diagnostic {
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
                    continue;
                }
                let AppRequestPayload::Interact(interaction) = envelope.payload() else {
                    panic!("expected conversation interaction");
                };
                assert_eq!(interaction.mode(), mode);
                assert_eq!(interaction.models(), &expected_models);
                assert_eq!(interaction.request().run_id(), expected_run);
                assert_eq!(interaction.request().workspace_id(), workspace);
                assert_eq!(interaction.request().providers(), expected_providers);
                if mode == ProductInteractionMode::Build {
                    assert!(interaction.request().task().contains("Immutable attachment snapshot"));
                    assert!(!interaction.request().task().contains("Later disk edits"));
                    assert!(
                        interaction.request().task().starts_with("Fixture message — unchanged")
                    );
                } else {
                    assert_eq!(interaction.request().task(), "Fixture message — unchanged");
                }
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
                )
                .unwrap();
                let observation = ProductInteractionSnapshot::new(
                    snapshot,
                    mode,
                    expected_models.clone(),
                    9_007_199_254_740_993,
                    9_007_199_254_740_992,
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
                let response = AppResponseEnvelope::new(
                    envelope.context(),
                    envelope.request_id(),
                    envelope.correlation_id(),
                    AppResponsePayload::Interaction(observation),
                );
                write_message(&mut stream, AppMessage::Response(response)).await;
                break;
            }
        }
    });
    for mode in ["chat", "plan", "review", "build"] {
        let input = json!({
            "operation":crate::state::id().unwrap(),"session":session, "text":"Fixture message — unchanged", "mode":mode,
            "attachments":if mode=="build"{vec![attachment["id"].clone()]}else{Vec::new()},
            "providers":{"writer":"","reviewer":hex(reviewer.as_bytes()),"fixer":""},
            "models":{
                "writer":{"id":"fixture-writer","manual":true,"effort":"high"},
                "reviewer":{"id":"fixture-reviewer","manual":false,"effort":"medium"}
            }
        });
        let output = tokio::time::timeout(Duration::from_secs(3), send(&app, &input))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(output["run"]["id"], session);
        assert_eq!(output["run"]["workspace"], hex(workspace.as_bytes()));
        assert_eq!(output["run"]["busy"], true);
        assert_eq!(output["received"], "9007199254740993");
        assert_eq!(output["incorporated"], "9007199254740992");
        assert_eq!(
            output["activities"][0],
            json!({"id":"7","kind":"status",
            "text":"Fixture receipt","detail":"Not yet incorporated"})
        );
    }
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
        vec![VersionRange::new(1, 0, 0).unwrap()],
        Vec::new(),
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
async fn lost_response_is_uncertain_and_never_reissued_after_restart() {
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    let endpoint = root.join("recovery.sock");
    let listener = UnixListener::bind(&endpoint).unwrap();
    let app = isolated_app(root, endpoint.clone());
    let server = tokio::spawn(async move {
        let (stream, request) = receive_request(&listener).await;
        assert!(matches!(request.payload(), AppRequestPayload::ControlProductRun(_)));
        drop(stream);
    });
    let payload = AppRequestPayload::ControlProductRun(ProductRunControl::new(
        RunId::new([1; 16]).unwrap(),
        ProductRunControlAction::Cancel,
    ));
    let error = receipts::recorded(&app, "original", payload.clone()).await.unwrap_err();
    assert!(error.1);
    server.await.unwrap();
    let reopened = isolated_app(root, endpoint);
    assert!(receipts::observed(&reopened, "original").unwrap().is_none());
    let retained = reopened.snapshot().unwrap();
    assert!(retained.operations["daemon:original"].input["frame"].as_str().unwrap().len() > 64);
    assert!(receipts::recorded(&reopened, "original", payload).await.unwrap_err().1);
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

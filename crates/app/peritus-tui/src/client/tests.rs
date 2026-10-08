use super::*;

#[tokio::test]
async fn reader_and_writer_failures_keep_their_originating_connection_identity() {
    let context = ProtocolContext::new(
        ProtocolId::new([12; 16]).unwrap(),
        peritus_app_protocol::ProtocolVersion::new(1, 0).unwrap(),
        SessionId::new([13; 16]).unwrap(),
    );
    let limits = AppProtocolLimits::PRODUCTION;
    let (events, mut receiver) = mpsc::channel(4);
    let (stream, peer) = tokio::io::duplex(64);
    drop(peer);
    reader_loop(stream, limits, context, events.clone()).await;
    let observed = receiver.recv().await.unwrap();
    assert_eq!(observed.context(), context);
    assert!(matches!(observed, ClientEvent::Disconnected { .. }));
    let (stream, peer) = tokio::io::duplex(64);
    drop(peer);
    let (sender, commands) = mpsc::channel(1);
    let hello = client_hello(context.protocol_id(), None, limits).unwrap();
    sender.send(WriterCommand::Message(AppMessage::ClientHello(hello))).await.unwrap();
    writer_loop(stream, limits, context, commands, events).await;
    let observed = receiver.recv().await.unwrap();
    assert_eq!(observed.context(), context);
    assert!(matches!(observed, ClientEvent::Disconnected { .. }));
}

#[tokio::test]
async fn dropping_a_client_aborts_both_owned_io_tasks() {
    let (session, _peer) = client_pair();
    let reader_abort = session.reader_task.abort_handle();
    let writer_abort = session.writer_task.abort_handle();
    drop(session);
    tokio::time::timeout(std::time::Duration::from_secs(1), async {
        while !reader_abort.is_finished() || !writer_abort.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("dropping the client must not detach a live reader or writer");
}

#[tokio::test]
async fn orderly_close_delivers_final_frames_before_releasing_the_stream() {
    let (session, mut peer) = client_pair();
    let limits = session.established.limits;
    let message = AppMessage::ClientHello(
        client_hello(session.established.context.protocol_id(), None, limits).unwrap(),
    );
    let expected = message.clone();
    let read = async {
        assert_eq!(read_frame(&mut peer, limits).await.unwrap(), expected);
        let mut byte = [0];
        assert_eq!(peer.read(&mut byte).await.unwrap(), 0, "writer closes after final frame");
    };
    let (result, ()) = tokio::join!(session.close(vec![message]), read);
    result.expect("orderly client shutdown");
}

fn client_pair() -> (ClientSession, tokio::io::DuplexStream) {
    let (stream, peer) = tokio::io::duplex(64);
    let (reader, writer) = tokio::io::split(stream);
    let limits = AppProtocolLimits::PRODUCTION;
    let context = ProtocolContext::new(
        ProtocolId::new([10; 16]).unwrap(),
        peritus_app_protocol::ProtocolVersion::new(1, 0).unwrap(),
        SessionId::new([11; 16]).unwrap(),
    );
    let (events, _receiver) = mpsc::channel(4);
    let (commands, command_rx) = mpsc::channel(4);
    let reader_task = tokio::spawn(reader_loop(reader, limits, context, events.clone()));
    let writer_task = tokio::spawn(writer_loop(writer, limits, context, command_rx, events));
    let session = ClientSession {
        established: EstablishedConnection {
            features: Vec::new(),
            context,
            limits,
            server: "fixture".into(),
            downgraded: false,
        },
        writer: commands,
        reader_task,
        writer_task,
    };
    (session, peer)
}

impl ClientSession {
    pub(crate) fn test_pair() -> (Self, tokio::io::DuplexStream) {
        client_pair()
    }
}

const COMMAND_FEATURES: [WellKnownProtocolFeature; 7] = [
    WellKnownProtocolFeature::ConversationLibrary,
    WellKnownProtocolFeature::ConversationForks,
    WellKnownProtocolFeature::WorkbenchExecution,
    WellKnownProtocolFeature::WorkbenchConversation,
    WellKnownProtocolFeature::WorkbenchContinuationReceipts,
    WellKnownProtocolFeature::WorkbenchRunBinding,
    WellKnownProtocolFeature::WorkbenchPreviewOutput,
];

fn command_features() -> Vec<ProtocolFeatureName> {
    COMMAND_FEATURES
        .into_iter()
        .map(|feature| ProtocolFeatureName::well_known(feature).expect("well-known feature"))
        .collect()
}

#[test]
fn client_hello_advertises_features_used_by_implemented_commands() {
    let hello = client_hello(
        ProtocolId::new([1; 16]).expect("protocol"),
        None,
        AppProtocolLimits::PRODUCTION,
    )
    .expect("client hello");

    for feature in command_features() {
        assert!(hello.optional_features().contains(&feature), "missing {feature:?}");
    }
}

#[test]
fn negotiation_selects_each_implemented_command_feature_when_the_daemon_advertises_it() {
    let protocol = ProtocolId::new([2; 16]).expect("protocol");
    let client = client_hello(protocol, None, AppProtocolLimits::PRODUCTION).expect("client hello");
    let server = peritus_app_protocol::ServerCapabilities::new(
        vec![peritus_app_protocol::CURRENT_PROTOCOL_RANGE],
        command_features(),
        AppProtocolLimits::PRODUCTION,
        "test-daemon".to_owned(),
    )
    .expect("server capabilities");
    let hello = peritus_app_protocol::negotiate(
        &client,
        &server,
        SessionId::new([3; 16]).expect("session"),
    )
    .expect("negotiation");
    let selected = match hello.outcome() {
        NegotiationOutcome::Compatible(protocol) | NegotiationOutcome::Downgraded(protocol) => {
            protocol.features()
        }
        NegotiationOutcome::Incompatible(reason) => panic!("incompatible: {reason:?}"),
    };

    for feature in command_features() {
        assert!(selected.contains(&feature), "feature was not negotiated: {feature:?}");
    }
}

#[test]
fn establishment_rejects_a_retired_version_even_if_the_server_marks_it_compatible() {
    let protocol = ProtocolId::new([4; 16]).expect("protocol");
    let retired = peritus_app_protocol::VersionRange::new(1, 0, 0).expect("retired version");
    let client = ClientHello::new(
        protocol,
        vec![retired],
        Vec::new(),
        Vec::new(),
        AppProtocolLimits::PRODUCTION,
        "retired-client".to_owned(),
    )
    .expect("retired client hello");
    let server = peritus_app_protocol::ServerCapabilities::new(
        vec![retired],
        Vec::new(),
        AppProtocolLimits::PRODUCTION,
        "retired-daemon".to_owned(),
    )
    .expect("retired server capabilities");
    let hello = peritus_app_protocol::negotiate(
        &client,
        &server,
        SessionId::new([5; 16]).expect("session"),
    )
    .expect("retired negotiation");

    let error = establish(protocol, &hello).expect_err("retired selection must be rejected");
    assert!(
        matches!(error, TuiError::ProtocolViolation(message) if message.contains("outside the offered range"))
    );
}

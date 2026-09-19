//! Socket-level checks of the public client contract.

#![cfg(unix)]

use std::time::Duration;

use peritus_app_client::{Client, ClientErrorKind, RequestIdentity};
use peritus_app_protocol::{
    AppMessage, AppProtocolLimits, AppRequestPayload, AppResponseEnvelope, AppResponsePayload,
    CorrelationId, OperationAcknowledgement, ProtocolContext, RequestId, ServerCapabilities,
    VersionRange, WellKnownProtocolFeature, decode_app_message, encode_app_message, negotiate,
};
use peritus_codec::HEADER_LEN;
use peritus_types::SessionId;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{UnixListener, UnixStream},
    sync::oneshot,
};

const DEADLINE: Duration = Duration::from_secs(3);

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

async fn handshake(stream: &mut UnixStream) {
    let AppMessage::ClientHello(hello) = read_message(stream).await else {
        panic!("expected client negotiation");
    };
    let capabilities = ServerCapabilities::new(
        vec![VersionRange::new(1, 0, 0).unwrap()],
        Vec::new(),
        AppProtocolLimits::PRODUCTION,
        "independent transport fixture".to_owned(),
    )
    .unwrap();
    let session = hello.requested_session().unwrap_or_else(|| SessionId::new([9; 16]).unwrap());
    let answer = negotiate(&hello, &capabilities, session).unwrap();
    write_message(stream, AppMessage::ServerHello(answer)).await;
}

fn endpoint() -> (tempfile::TempDir, std::path::PathBuf, UnixListener) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("daemon.sock");
    let listener = UnixListener::bind(&path).unwrap();
    (directory, path, listener)
}

#[tokio::test]
async fn matching_responses_allow_sequential_requests_on_the_same_connection() {
    let (_directory, path, listener) = endpoint();
    let session = SessionId::new([7; 16]).unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        handshake(&mut stream).await;
        for _ in 0..2 {
            let AppMessage::Request(request) = read_message(&mut stream).await else {
                panic!("expected request");
            };
            assert_eq!(request.context().session_id(), session);
            let answer = AppResponseEnvelope::new(
                request.context(),
                request.request_id(),
                request.correlation_id(),
                AppResponsePayload::Acknowledged(OperationAcknowledgement::new(
                    request.request_id(),
                )),
            );
            write_message(&mut stream, AppMessage::Response(answer)).await;
        }
    });
    let mut client = Client::connect(path.as_os_str(), Some(session), DEADLINE, &[]).await.unwrap();
    for _ in 0..2 {
        let identity = RequestIdentity::generate().unwrap();
        let answer = client.request(identity, AppRequestPayload::DaemonStatus).await.unwrap();
        assert_eq!(answer.request_id(), identity.request_id);
        assert_eq!(answer.correlation_id(), identity.correlation_id);
        assert!(client.is_usable());
    }
    tokio::time::timeout(DEADLINE, server).await.unwrap().unwrap();
}

#[tokio::test]
async fn missing_required_capability_fails_negotiation() {
    let (_directory, path, listener) = endpoint();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        handshake(&mut stream).await;
    });
    let result = Client::connect(
        path.as_os_str(),
        None,
        DEADLINE,
        &[WellKnownProtocolFeature::TerminalStreaming],
    )
    .await;
    let error = result.err().expect("missing capability must be rejected");
    assert_eq!(error.kind(), ClientErrorKind::Negotiation);
    tokio::time::timeout(DEADLINE, server).await.unwrap().unwrap();
}

async fn mismatched_response(field: &'static str) {
    let (_directory, path, listener) = endpoint();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        handshake(&mut stream).await;
        let AppMessage::Request(request) = read_message(&mut stream).await else {
            panic!("expected request");
        };
        let context = request.context();
        let context = if field == "context" {
            ProtocolContext::new(
                context.protocol_id(),
                context.version(),
                SessionId::new([8; 16]).unwrap(),
            )
        } else {
            context
        };
        let request_id = if field == "request" {
            RequestId::new([2; 16]).unwrap()
        } else {
            request.request_id()
        };
        let correlation_id = if field == "correlation" {
            CorrelationId::new([3; 16]).unwrap()
        } else {
            request.correlation_id()
        };
        let response = AppResponseEnvelope::new(
            context,
            request_id,
            correlation_id,
            AppResponsePayload::Acknowledged(OperationAcknowledgement::new(request_id)),
        );
        write_message(&mut stream, AppMessage::Response(response)).await;
    });
    let mut client = Client::connect(path.as_os_str(), None, DEADLINE, &[]).await.unwrap();
    let identity = RequestIdentity::new(
        RequestId::new([4; 16]).unwrap(),
        CorrelationId::new([5; 16]).unwrap(),
    );
    let error = client.request(identity, AppRequestPayload::DaemonStatus).await.unwrap_err();
    assert_eq!(error.kind(), ClientErrorKind::Protocol);
    assert!(!client.is_usable());
    let error = client.request(identity, AppRequestPayload::DaemonStatus).await.unwrap_err();
    assert_eq!(error.kind(), ClientErrorKind::Connection);
    tokio::time::timeout(DEADLINE, server).await.unwrap().unwrap();
}

#[tokio::test]
async fn foreign_request_identity_invalidates_connection() {
    mismatched_response("request").await;
}

#[tokio::test]
async fn foreign_correlation_identity_invalidates_connection() {
    mismatched_response("correlation").await;
}

#[tokio::test]
async fn foreign_protocol_context_invalidates_connection() {
    mismatched_response("context").await;
}

#[tokio::test]
async fn cancelling_an_in_flight_request_prevents_redispatch_on_the_stream() {
    let (_directory, path, listener) = endpoint();
    let (received_tx, received_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        handshake(&mut stream).await;
        assert!(matches!(read_message(&mut stream).await, AppMessage::Request(_)));
        received_tx.send(()).unwrap();
        release_rx.await.unwrap();
        let mut byte = [0];
        assert_eq!(stream.read(&mut byte).await.unwrap(), 0, "no second request may be sent");
    });
    let mut client = Client::connect(path.as_os_str(), None, DEADLINE, &[]).await.unwrap();
    let identity = RequestIdentity::generate().unwrap();
    {
        let request = client.request(identity, AppRequestPayload::DaemonStatus);
        tokio::pin!(request);
        tokio::select! {
            result = &mut request => panic!("fixture must keep the request pending: {result:?}"),
            received = received_rx => received.unwrap(),
            () = tokio::time::sleep(DEADLINE) => panic!("fixture did not receive the request"),
        }
    }
    assert!(!client.is_usable());
    let error = client.request(identity, AppRequestPayload::DaemonStatus).await.unwrap_err();
    assert_eq!(error.kind(), ClientErrorKind::Connection);
    drop(client);
    release_tx.send(()).unwrap();
    tokio::time::timeout(DEADLINE, server).await.unwrap().unwrap();
}

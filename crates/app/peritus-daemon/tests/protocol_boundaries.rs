//! Black-box negotiation and pre-allocation frame-boundary tests.

#![cfg(unix)]

mod support;

use std::{future::Future, io, path::Path, time::Duration};

use peritus_app_protocol::{
    APP_SCHEMA_V1, AppMessage, AppProtocolLimits, CLIENT_HELLO_FAMILY, ClientHello,
    IncompatibilityReason, NegotiationOutcome, ProtocolId, VersionRange, encode_app_message,
};
use peritus_codec::{FORMAT_VERSION, HEADER_LEN, MAGIC};
use peritus_daemon::{AppFrameStream, DaemonErrorCode, DaemonRuntime, LocalEndpointAddress};
use tempfile::TempDir;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::UnixStream,
    runtime::Builder,
};

const IO_BOUND: Duration = Duration::from_secs(5);
const LIFECYCLE_BOUND: Duration = Duration::from_secs(10);

#[test]
fn incompatible_hello_returns_no_session_and_closes_the_connection() {
    run_async_test(incompatible_hello_returns_no_session_and_closes_the_connection_async());
}

async fn incompatible_hello_returns_no_session_and_closes_the_connection_async() {
    let temporary = TempDir::new().expect("temporary root");
    let runtime = tokio::time::timeout(
        LIFECYCLE_BOUND,
        DaemonRuntime::start(support::configuration(temporary.path())),
    )
    .await
    .expect("daemon startup completes within the bound")
    .expect("daemon starts");
    let socket = unix_address(&runtime);
    let stream = connect(&socket).await;
    let mut frames = AppFrameStream::new(stream, AppProtocolLimits::PRODUCTION);
    let client = ClientHello::new(
        ProtocolId::new([71; 16]).expect("protocol identity"),
        vec![VersionRange::new(2, 0, 0).expect("unsupported version")],
        Vec::new(),
        Vec::new(),
        AppProtocolLimits::PRODUCTION,
        "peritus-incompatible-test".to_owned(),
    )
    .expect("incompatible client hello");

    tokio::time::timeout(IO_BOUND, frames.write(&AppMessage::ClientHello(client)))
        .await
        .expect("hello write completes within the bound")
        .expect("write incompatible hello");
    let AppMessage::ServerHello(server) = tokio::time::timeout(IO_BOUND, frames.read())
        .await
        .expect("server hello arrives within the bound")
        .expect("read incompatible server hello")
    else {
        panic!("daemon did not answer with ServerHello");
    };
    assert_eq!(
        server.outcome(),
        &NegotiationOutcome::Incompatible(IncompatibilityReason::NoCommonVersion),
    );
    assert_eq!(server.established_session(), None);

    let closed = tokio::time::timeout(IO_BOUND, frames.read())
        .await
        .expect("incompatible connection closes within the bound")
        .expect_err("incompatible connection must not accept post-hello frames");
    assert_eq!(closed.code_kind(), DaemonErrorCode::Transport);
    tokio::time::timeout(LIFECYCLE_BOUND, runtime.shutdown())
        .await
        .expect("daemon shutdown completes within the bound")
        .expect("daemon shuts down cleanly");
}

#[test]
fn malformed_and_oversized_headers_close_without_waiting_for_declared_payloads() {
    run_async_test(
        malformed_and_oversized_headers_close_without_waiting_for_declared_payloads_async(),
    );
}

async fn malformed_and_oversized_headers_close_without_waiting_for_declared_payloads_async() {
    let temporary = TempDir::new().expect("temporary root");
    let runtime = tokio::time::timeout(
        LIFECYCLE_BOUND,
        DaemonRuntime::start(support::configuration(temporary.path())),
    )
    .await
    .expect("daemon startup completes within the bound")
    .expect("daemon starts");
    let socket = unix_address(&runtime);
    let valid = encode_app_message(
        &AppMessage::ClientHello(compatible_hello(72)),
        AppProtocolLimits::PRODUCTION,
    )
    .expect("valid hello frame");

    let mut bad_magic = valid[..HEADER_LEN].to_vec();
    bad_magic[..MAGIC.len()].copy_from_slice(b"NOPE");
    assert_connection_rejected(&socket, &bad_magic).await;

    let mut nonzero_flags = valid.clone();
    nonzero_flags[10..12].copy_from_slice(&1_u16.to_be_bytes());
    assert_connection_rejected(&socket, &nonzero_flags).await;

    let oversized_payload = u32::try_from(AppProtocolLimits::PRODUCTION.codec().max_payload_bytes)
        .expect("production payload bound fits u32")
        .checked_add(1)
        .expect("oversized boundary fits u32");
    let oversized_header = frame_header(oversized_payload);
    assert_connection_rejected(&socket, &oversized_header).await;

    tokio::time::timeout(LIFECYCLE_BOUND, runtime.shutdown())
        .await
        .expect("daemon shutdown completes within the bound")
        .expect("daemon shuts down cleanly");
}

fn run_async_test(test: impl Future<Output = ()>) {
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build current-thread test runtime");
    runtime.block_on(test);
}

fn compatible_hello(identity: u8) -> ClientHello {
    ClientHello::new(
        ProtocolId::new([identity; 16]).expect("protocol identity"),
        vec![VersionRange::new(1, 0, 0).expect("version")],
        Vec::new(),
        Vec::new(),
        AppProtocolLimits::PRODUCTION,
        "peritus-framing-test".to_owned(),
    )
    .expect("client hello")
}

fn unix_address(runtime: &DaemonRuntime) -> std::path::PathBuf {
    let LocalEndpointAddress::Unix(socket) = runtime.endpoint_address().clone();
    socket
}

async fn connect(socket: &Path) -> UnixStream {
    tokio::time::timeout(IO_BOUND, UnixStream::connect(socket))
        .await
        .expect("socket connection completes within the bound")
        .expect("connect protected socket")
}

async fn assert_connection_rejected(socket: &Path, bytes: &[u8]) {
    let mut stream = connect(socket).await;
    tokio::time::timeout(IO_BOUND, stream.write_all(bytes))
        .await
        .expect("malformed write completes within the bound")
        .expect("write malformed frame");
    let mut observed = [0_u8; 1];
    match tokio::time::timeout(IO_BOUND, stream.read(&mut observed))
        .await
        .expect("malformed connection closes within the bound")
    {
        Ok(0) => {}
        Err(error) if connection_closed(&error) => {}
        Ok(count) => panic!("malformed connection returned {count} unexpected byte(s)"),
        Err(error) => panic!("malformed connection failed unexpectedly: {error}"),
    }
}

fn connection_closed(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::NotConnected
            | io::ErrorKind::UnexpectedEof
    )
}

fn frame_header(payload_len: u32) -> [u8; HEADER_LEN] {
    let mut header = [0_u8; HEADER_LEN];
    header[..4].copy_from_slice(&MAGIC);
    header[4..6].copy_from_slice(&FORMAT_VERSION.to_be_bytes());
    header[6..8].copy_from_slice(&CLIENT_HELLO_FAMILY.to_be_bytes());
    header[8..10].copy_from_slice(&APP_SCHEMA_V1.to_be_bytes());
    header[10..12].copy_from_slice(&0_u16.to_be_bytes());
    header[12..16].copy_from_slice(&payload_len.to_be_bytes());
    header
}

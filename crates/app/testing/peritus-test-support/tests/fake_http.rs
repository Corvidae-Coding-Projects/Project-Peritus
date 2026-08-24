//! Deterministic loopback HTTP server behavior and lifecycle tests.

use peritus_test_support::{
    ExpectedHttpRequest, FakeHttpErrorKind, FakeHttpExchangeScript, FakeHttpFault, FakeHttpHeader,
    FakeHttpLimits, FakeHttpReleasePoint, FakeHttpSequenceServer, FakeHttpServer,
    FakeHttpTermination, HeaderMatchMode, ScriptedHttpResponse,
};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{Shutdown, SocketAddr, TcpStream};
use std::time::Duration;

const HEADER_CANARY: &str = "bearer-header-canary";
const TARGET_CANARY: &str = "target-canary";
const BODY_CANARY: &[u8] = b"private-body";

fn header(name: &str, value: impl Into<Vec<u8>>) -> FakeHttpHeader {
    FakeHttpHeader::new(name, value).expect("test header must be valid")
}

fn expectation(target: &str, body: &[u8], limits: FakeHttpLimits) -> ExpectedHttpRequest {
    ExpectedHttpRequest::new(
        "POST",
        target,
        vec![
            header("Host", b"fixture"),
            header("Authorization", format!("Bearer {HEADER_CANARY}")),
            header("Content-Length", body.len().to_string()),
        ],
        body,
        limits,
    )
    .expect("test expectation must be valid")
}

fn response(
    chunks: &[&[u8]],
    fault: FakeHttpFault,
    release: Option<FakeHttpReleasePoint>,
    limits: FakeHttpLimits,
) -> ScriptedHttpResponse {
    let content_length: usize = chunks.iter().map(|chunk| chunk.len()).sum();
    ScriptedHttpResponse::new(
        201,
        vec![header("Content-Length", content_length.to_string()), header("X-Fixture", b"generic")],
        chunks.iter().map(|chunk| chunk.to_vec()).collect(),
        fault,
        release,
        limits,
    )
    .expect("test response must be valid")
}

fn send_request(address: SocketAddr, target: &str, body: &[u8]) -> TcpStream {
    let mut stream = TcpStream::connect(address).expect("loopback connection must open");
    write!(
        stream,
        "POST {target} HTTP/1.1\r\nHost: fixture\r\nAuthorization: Bearer {HEADER_CANARY}\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .expect("request head must write");
    stream.write_all(body).expect("request body must write");
    stream
}

fn read_all(mut stream: TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes).expect("scripted response must be readable");
    bytes
}

#[test]
fn exact_request_and_chunk_script_produce_redacted_bounded_observation() {
    let limits = FakeHttpLimits::default();
    let target = format!("/submit?key={TARGET_CANARY}");
    let expected = expectation(&target, BODY_CANARY, limits);
    let scripted = response(&[BODY_CANARY], FakeHttpFault::Complete, None, limits);
    let fixture_debug = format!("{expected:?} {scripted:?}");
    assert!(!fixture_debug.contains(HEADER_CANARY));
    assert!(!fixture_debug.contains(TARGET_CANARY));
    assert!(!fixture_debug.contains(std::str::from_utf8(BODY_CANARY).expect("canary is UTF-8")));
    let server = FakeHttpServer::start(expected, scripted, limits).expect("server must start");
    assert!(server.base_url().starts_with("http://127.0.0.1:"));

    let bytes = read_all(send_request(server.address(), &target, BODY_CANARY));
    assert!(bytes.starts_with(b"HTTP/1.1 201 Created\r\n"));
    assert!(bytes.ends_with(BODY_CANARY));

    let exchange = server.finish().expect("exchange must finish");
    assert!(exchange.request().matched());
    assert_eq!(exchange.request().method(), "POST");
    assert_eq!(exchange.request().target_bytes(), target.len());
    assert_eq!(exchange.request().target_sha256(), digest(target.as_bytes()));
    assert_eq!(exchange.request().body_bytes(), BODY_CANARY.len());
    assert_eq!(exchange.request().body_sha256(), digest(BODY_CANARY));
    assert_eq!(exchange.chunks_sent(), 1);
    assert_eq!(exchange.termination(), FakeHttpTermination::Completed);
    let authorization = exchange
        .request()
        .headers()
        .iter()
        .find(|captured| captured.name() == "authorization")
        .expect("authorization observation must be present");
    assert!(authorization.is_sensitive());
    assert_eq!(authorization.value_bytes(), "Bearer ".len() + HEADER_CANARY.len());
    assert_eq!(authorization.value_sha256(), digest(format!("Bearer {HEADER_CANARY}").as_bytes()));

    let debug = format!("{exchange:?}");
    assert!(!debug.contains(HEADER_CANARY));
    assert!(!debug.contains(TARGET_CANARY));
    assert!(!debug.contains(std::str::from_utf8(BODY_CANARY).expect("canary is UTF-8")));
}

#[test]
fn request_mismatch_is_a_direct_observation_and_does_not_send_the_script() {
    let limits = FakeHttpLimits::default();
    let expected = expectation("/expected", b"right", limits)
        .header_match_mode(HeaderMatchMode::AllowAdditional);
    let scripted = response(&[b"must-not-send"], FakeHttpFault::Complete, None, limits);
    let server = FakeHttpServer::start(expected, scripted, limits).expect("server must start");
    let bytes = read_all(send_request(server.address(), "/expected", b"wrong"));
    assert!(bytes.starts_with(b"HTTP/1.1 400 Bad Request\r\n"));
    let exchange = server.finish().expect("mismatch remains an observation");
    assert!(!exchange.request().matched());
    assert_eq!(exchange.chunks_sent(), 0);
}

#[test]
fn deliberate_close_stops_at_the_exact_scripted_fault_point() {
    let limits = FakeHttpLimits::default();
    let expected = expectation("/stream", b"go", limits);
    let scripted =
        response(&[b"aa", b"bb", b"cc"], FakeHttpFault::CloseAfterChunks(2), None, limits);
    let server = FakeHttpServer::start(expected, scripted, limits).expect("server must start");
    let bytes = read_all(send_request(server.address(), "/stream", b"go"));
    assert!(bytes.ends_with(b"\r\n\r\naabb"));
    let exchange = server.finish().expect("faulted exchange must finish");
    assert_eq!(exchange.chunks_sent(), 2);
    assert_eq!(exchange.termination(), FakeHttpTermination::ScriptedClose);
}

#[test]
fn ordered_sequence_uses_one_endpoint_and_returns_every_exchange() {
    let limits = FakeHttpLimits::default();
    let scripts = [
        ("/retry", b"one".as_slice(), b"limited".as_slice()),
        ("/retry", b"two".as_slice(), b"complete".as_slice()),
    ]
    .into_iter()
    .map(|(target, body, output)| {
        FakeHttpExchangeScript::new(
            expectation(target, body, limits),
            response(&[output], FakeHttpFault::Complete, None, limits),
        )
    })
    .collect();
    let server =
        FakeHttpSequenceServer::start(scripts, limits).expect("sequence server must start");
    let address = server.address();
    let first = read_all(send_request(address, "/retry", b"one"));
    let second = read_all(send_request(address, "/retry", b"two"));
    assert!(first.ends_with(b"limited"));
    assert!(second.ends_with(b"complete"));
    let exchanges = server.finish().expect("sequence must finish");
    assert_eq!(exchanges.len(), 2);
    assert!(exchanges.iter().all(|exchange| exchange.request().matched()));
    assert!(
        exchanges.iter().all(|exchange| exchange.termination() == FakeHttpTermination::Completed)
    );
}

#[test]
fn release_control_makes_peer_cancellation_observable_without_polling() {
    let limits = FakeHttpLimits::default();
    let expected = expectation("/cancel", b"wait", limits);
    let scripted = response(
        &[b"late"],
        FakeHttpFault::Complete,
        Some(FakeHttpReleasePoint::BeforeHeaders),
        limits,
    );
    let server = FakeHttpServer::start(expected, scripted, limits).expect("server must start");
    let client = send_request(server.address(), "/cancel", b"wait");
    assert_eq!(
        server.wait_until_blocked(Duration::from_secs(2)).expect("worker must pause"),
        FakeHttpReleasePoint::BeforeHeaders
    );
    client.shutdown(Shutdown::Both).expect("client cancellation must close the socket");
    drop(client);
    server.release().expect("paused worker must release");
    let exchange = server.finish().expect("cancelled exchange must finish");
    assert_eq!(exchange.termination(), FakeHttpTermination::PeerClosed);
    assert_eq!(exchange.chunks_sent(), 0);
}

#[test]
fn drop_joins_workers_blocked_in_accept_read_and_release() {
    let limits = FakeHttpLimits::default();
    let scripted = response(&[b"ok"], FakeHttpFault::Complete, None, limits);
    let accept_server =
        FakeHttpServer::start(expectation("/accept", b"go", limits), scripted.clone(), limits)
            .expect("accept server must start");
    drop(accept_server);

    let read_server = FakeHttpServer::start(expectation("/read", b"go", limits), scripted, limits)
        .expect("read server must start");
    let _partial = TcpStream::connect(read_server.address()).expect("partial client must connect");
    drop(read_server);

    let paused_server = FakeHttpServer::start(
        expectation("/pause", b"go", limits),
        response(
            &[b"ok"],
            FakeHttpFault::Complete,
            Some(FakeHttpReleasePoint::BeforeHeaders),
            limits,
        ),
        limits,
    )
    .expect("paused server must start");
    let _client = send_request(paused_server.address(), "/pause", b"go");
    paused_server
        .wait_until_blocked(Duration::from_secs(2))
        .expect("paused server must reach release point");
    drop(paused_server);

    let sequence_server = FakeHttpSequenceServer::start(
        vec![
            FakeHttpExchangeScript::new(
                expectation("/sequence-one", b"go", limits),
                response(&[b"one"], FakeHttpFault::Complete, None, limits),
            ),
            FakeHttpExchangeScript::new(
                expectation("/sequence-two", b"go", limits),
                response(&[b"two"], FakeHttpFault::Complete, None, limits),
            ),
        ],
        limits,
    )
    .expect("sequence server must start");
    drop(sequence_server);
}

#[test]
fn configuration_and_incoming_request_limits_fail_explicitly() {
    assert_eq!(
        FakeHttpLimits::new(0, 1, 1, 1, 1).expect_err("zero limit must fail").kind(),
        FakeHttpErrorKind::InvalidConfiguration
    );
    assert_eq!(
        FakeHttpSequenceServer::start(Vec::new(), FakeHttpLimits::default())
            .expect_err("empty sequence must fail")
            .kind(),
        FakeHttpErrorKind::InvalidConfiguration
    );
    let limits = FakeHttpLimits::new(96, 4, 4, 4, 2).expect("small limits must be valid");
    assert_eq!(
        ExpectedHttpRequest::new("POST", "/", Vec::new(), b"12345", limits)
            .expect_err("oversized expectation must fail")
            .kind(),
        FakeHttpErrorKind::InvalidConfiguration
    );
    assert_eq!(
        ScriptedHttpResponse::new(
            200,
            Vec::new(),
            vec![b"12345".to_vec()],
            FakeHttpFault::Complete,
            None,
            limits,
        )
        .expect_err("oversized chunk must fail")
        .kind(),
        FakeHttpErrorKind::InvalidConfiguration
    );
    assert_eq!(
        ScriptedHttpResponse::new(
            200,
            Vec::new(),
            vec![b"ok".to_vec()],
            FakeHttpFault::Complete,
            Some(FakeHttpReleasePoint::BeforeClose),
            limits,
        )
        .expect_err("unreachable release point must fail")
        .kind(),
        FakeHttpErrorKind::InvalidConfiguration
    );

    let expected = ExpectedHttpRequest::new("GET", "/", Vec::new(), Vec::new(), limits)
        .expect("small expectation must be valid");
    let scripted = ScriptedHttpResponse::new(
        204,
        Vec::new(),
        Vec::new(),
        FakeHttpFault::Complete,
        None,
        limits,
    )
    .expect("empty response must be valid");
    let server = FakeHttpServer::start(expected, scripted, limits).expect("server must start");
    let mut client = TcpStream::connect(server.address()).expect("client must connect");
    write!(client, "GET / HTTP/1.1\r\nX-Oversized: {}", "x".repeat(128))
        .expect("oversized request must reach server");
    client.shutdown(Shutdown::Both).expect("client must close");
    assert_eq!(
        server.finish().expect_err("request head limit must fail").kind(),
        FakeHttpErrorKind::RequestLimit
    );
}

fn digest(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

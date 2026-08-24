//! Loopback tests for the dependency-private Reqwest/Rustls transport.

#[path = "support/runtime.rs"]
mod runtime;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use peritus_provider_core::{
    CancellationToken, Endpoint, HttpHeaders, HttpLimits, HttpMethod, HttpRequest, HttpTransport,
    ProviderCoreErrorKind, ReqwestTransport,
};

fn serve_once(response: &'static [u8]) -> (Endpoint, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake server");
    let address = listener.local_addr().expect("fake server address");
    let endpoint = Endpoint::new(format!("http://{address}/v1/responses")).expect("endpoint");
    let worker = thread::spawn(move || {
        let (mut socket, _) = listener.accept().expect("accept one request");
        socket.set_read_timeout(Some(Duration::from_secs(2))).expect("read timeout");
        let mut request = [0_u8; 4096];
        let count = socket.read(&mut request).expect("read request");
        assert!(request[..count].windows(4).any(|window| window == b"\r\n\r\n"));
        socket.write_all(response).expect("write response");
    });
    (endpoint, worker)
}

#[test]
fn default_transport_does_not_follow_redirects_and_streams_owned_bytes() {
    runtime::block_on(async {
        let (endpoint, worker) = serve_once(
            b"HTTP/1.1 302 Found\r\nLocation: http://127.0.0.1:1/forbidden\r\nX-Request-Id: provider-canary\r\nContent-Length: 6\r\nConnection: close\r\n\r\nh\xc3\xa9llo",
        );
        let limits = HttpLimits::new([16, 2048, 1024, 1024, 1024]).expect("limits");
        let request =
            HttpRequest::new(HttpMethod::Get, endpoint, HttpHeaders::empty(), Vec::new(), limits)
                .expect("request");
        let transport: Box<dyn HttpTransport> =
            Box::new(ReqwestTransport::new(limits).expect("transport"));
        let cancellation = CancellationToken::new();
        let response = transport.send(request, &cancellation).await.expect("response");
        assert_eq!(response.status().as_u16(), 302);
        assert_eq!(
            response
                .headers()
                .first("x-request-id")
                .expect("request id")
                .nonsensitive_bytes()
                .expect("request ID response header is nonsensitive"),
            b"provider-canary"
        );
        let (_, _, mut body) = response.into_parts();
        let mut bytes = Vec::new();
        while let Some(chunk) = body.next(&cancellation).await.expect("body chunk") {
            bytes.extend_from_slice(&chunk);
        }
        assert_eq!(bytes, "héllo".as_bytes());
        worker.join().expect("fake server joined");
    });
}

#[test]
fn default_transport_rejects_oversized_content_length_before_body_reads() {
    runtime::block_on(async {
        let (endpoint, worker) = serve_once(
            b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\n123456789",
        );
        let limits = HttpLimits::new([8, 1024, 8, 8, 8]).expect("limits");
        let request =
            HttpRequest::new(HttpMethod::Get, endpoint, HttpHeaders::empty(), Vec::new(), limits)
                .expect("request");
        let transport = ReqwestTransport::new(limits).expect("transport");
        let error = transport
            .send(request, &CancellationToken::new())
            .await
            .expect_err("content length bound");
        assert_eq!(error.kind(), ProviderCoreErrorKind::LimitExceeded);
        worker.join().expect("fake server joined");
    });
}

#[test]
fn default_transport_honors_pre_send_cancellation() {
    runtime::block_on(async {
        let limits = HttpLimits::new([8, 1024, 8, 8, 8]).expect("limits");
        let request = HttpRequest::new(
            HttpMethod::Get,
            Endpoint::new("http://127.0.0.1:9/cancelled".to_owned()).expect("endpoint"),
            HttpHeaders::empty(),
            Vec::new(),
            limits,
        )
        .expect("request");
        let transport = ReqwestTransport::new(limits).expect("transport");
        let cancellation = CancellationToken::new();
        assert!(cancellation.cancel());
        let error = transport.send(request, &cancellation).await.expect_err("cancelled send");
        assert_eq!(error.kind(), ProviderCoreErrorKind::Cancelled);
    });
}

#[test]
fn default_transport_classifies_connection_refusal_before_submission() {
    runtime::block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").expect("reserve loopback port");
        let address = listener.local_addr().expect("reserved address");
        drop(listener);
        let limits = HttpLimits::new([8, 1024, 8, 8, 8]).expect("limits");
        let request = HttpRequest::new(
            HttpMethod::Get,
            Endpoint::new(format!("http://{address}/connect-failure")).expect("endpoint"),
            HttpHeaders::empty(),
            Vec::new(),
            limits,
        )
        .expect("request");
        let transport = ReqwestTransport::new(limits).expect("transport");
        let error = transport
            .send(request, &CancellationToken::new())
            .await
            .expect_err("connection refusal");
        assert_eq!(error.kind(), ProviderCoreErrorKind::Connect);
    });
}

#[test]
fn default_transport_interrupts_a_pending_body_read() {
    runtime::block_on(async {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake server");
        let address = listener.local_addr().expect("fake server address");
        let endpoint = Endpoint::new(format!("http://{address}/pending-body")).expect("endpoint");
        let (release_sender, release_receiver) = mpsc::channel();
        let worker = thread::spawn(move || {
            let (mut socket, _) = listener.accept().expect("accept request");
            socket.set_read_timeout(Some(Duration::from_secs(2))).expect("read timeout");
            let mut request = [0_u8; 4096];
            let _ = socket.read(&mut request).expect("read request");
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1\r\nConnection: close\r\n\r\n")
                .expect("write headers");
            release_receiver.recv_timeout(Duration::from_secs(2)).expect("release server");
        });

        let limits = HttpLimits::new([8, 1024, 8, 8, 8]).expect("limits");
        let request =
            HttpRequest::new(HttpMethod::Get, endpoint, HttpHeaders::empty(), Vec::new(), limits)
                .expect("request");
        let transport = ReqwestTransport::new(limits).expect("transport");
        let cancellation = CancellationToken::new();
        let response = transport.send(request, &cancellation).await.expect("response headers");
        let (_, _, mut body) = response.into_parts();
        let reader_cancellation = cancellation.clone();
        let reader = tokio::spawn(async move { body.next(&reader_cancellation).await });
        tokio::task::yield_now().await;
        assert!(cancellation.cancel());
        let error =
            reader.await.expect("owned reader completion").expect_err("pending read cancellation");
        assert_eq!(error.kind(), ProviderCoreErrorKind::Cancelled);
        release_sender.send(()).expect("release server");
        worker.join().expect("fake server joined");
    });
}

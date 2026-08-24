//! Managed proxy redirect tests.

use super::*;

#[test]
fn managed_proxy_revalidates_and_suppresses_a_denied_absolute_redirect() {
    let _guard = serial_proxy_test();
    let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = upstream.local_addr().unwrap().port();
    let upstream_task = thread::spawn(move || {
        let (mut stream, _) = upstream.accept().unwrap();
        let _ = read_head(&mut stream);
        write!(
            stream,
            "HTTP/1.1 302 Found\r\nLocation: http://denied.test:{port}/next\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
    });
    let plan = runtime_plan(
        vec![
            rule(RuleEffect::Allow, HostMatcher::DnsExact(dns("loop.test")), port),
            rule(
                RuleEffect::Allow,
                HostMatcher::ip_prefix(IpAddr::V4(Ipv4Addr::LOCALHOST), 32).unwrap(),
                port,
            ),
        ],
        "loop.test",
        port,
        Vec::new(),
    );
    let proxy = ManagedProxy::start_with(
        plan,
        RoutingToken::new([13; 32]),
        Arc::new(LoopbackResolver),
        None,
    )
    .unwrap();
    let mut client = TcpStream::connect(proxy.endpoint().socket_addr()).unwrap();
    proxy.routing_token().expose_header(|token| {
        write!(
            client,
            "GET http://loop.test:{port}/start HTTP/1.1\r\nHost: loop.test:{port}\r\nProxy-Authorization: Peritus {token}\r\n\r\n"
        )
        .unwrap();
    });
    let response = read_to_close(&mut client);
    assert!(response.is_empty());
    upstream_task.join().unwrap();
    wait_for_closed(&proxy, ConnectionDecision::Failed);
    assert!(proxy.shutdown().unwrap().workers_joined());
}

#[test]
fn managed_proxy_follows_relative_redirect_and_returns_only_final_response() {
    let _guard = serial_proxy_test();
    let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = upstream.local_addr().unwrap().port();
    let upstream_task = thread::spawn(move || {
        let (mut first, _) = upstream.accept().unwrap();
        assert!(read_head(&mut first).contains("GET /start HTTP/1.1"));
        first
            .write_all(
                b"HTTP/1.1 302 Found\r\nLocation: /final\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            )
            .unwrap();
        let (mut second, _) = upstream.accept().unwrap();
        assert!(read_head(&mut second).contains("GET /final HTTP/1.1"));
        second
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK")
            .unwrap();
    });
    let plan = runtime_plan(
        vec![
            rule(RuleEffect::Allow, HostMatcher::DnsExact(dns("loop.test")), port),
            rule(
                RuleEffect::Allow,
                HostMatcher::ip_prefix(IpAddr::V4(Ipv4Addr::LOCALHOST), 32).unwrap(),
                port,
            ),
        ],
        "loop.test",
        port,
        Vec::new(),
    );
    let proxy = ManagedProxy::start_with(
        plan,
        RoutingToken::new([16; 32]),
        Arc::new(LoopbackResolver),
        None,
    )
    .unwrap();
    let mut client = TcpStream::connect(proxy.endpoint().socket_addr()).unwrap();
    proxy.routing_token().expose_header(|token| {
        write!(
            client,
            "GET http://loop.test:{port}/start HTTP/1.1\r\nHost: loop.test:{port}\r\nProxy-Authorization: Peritus {token}\r\n\r\n"
        )
        .unwrap();
    });
    let response = String::from_utf8(read_to_close(&mut client)).unwrap();
    assert!(response.starts_with("HTTP/1.1 200 OK"));
    assert!(response.ends_with("OK"));
    upstream_task.join().unwrap();
    assert!(proxy.shutdown().unwrap().workers_joined());
}

#[test]
fn managed_proxy_preserves_redirect_count_across_upstream_connections() {
    let _guard = serial_proxy_test();
    let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = upstream.local_addr().unwrap().port();
    let upstream_task = thread::spawn(move || {
        for next in ["/one", "/two"] {
            let (mut stream, _) = upstream.accept().unwrap();
            let _ = read_head(&mut stream);
            write!(
                stream,
                "HTTP/1.1 302 Found\r\nLocation: {next}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            )
            .unwrap();
        }
    });
    let plan = runtime_plan(
        vec![
            rule(RuleEffect::Allow, HostMatcher::DnsExact(dns("loop.test")), port),
            rule(
                RuleEffect::Allow,
                HostMatcher::ip_prefix(IpAddr::V4(Ipv4Addr::LOCALHOST), 32).unwrap(),
                port,
            ),
        ],
        "loop.test",
        port,
        Vec::new(),
    );
    let proxy = ManagedProxy::start_with(
        plan,
        RoutingToken::new([17; 32]),
        Arc::new(LoopbackResolver),
        None,
    )
    .unwrap();
    let mut client = TcpStream::connect(proxy.endpoint().socket_addr()).unwrap();
    proxy.routing_token().expose_header(|token| {
        write!(
            client,
            "GET http://loop.test:{port}/start HTTP/1.1\r\nHost: loop.test:{port}\r\nProxy-Authorization: Peritus {token}\r\n\r\n"
        )
        .unwrap();
    });
    let response = read_to_close(&mut client);
    assert!(response.is_empty());
    upstream_task.join().unwrap();
    wait_for_closed(&proxy, ConnectionDecision::Failed);
    assert!(proxy.shutdown().unwrap().workers_joined());
}

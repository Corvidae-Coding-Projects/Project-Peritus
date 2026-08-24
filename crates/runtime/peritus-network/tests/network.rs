//! Managed-network plan, matching, credential, redirect, proxy, and teardown tests.

use base64::Engine;
#[cfg(unix)]
use std::os::fd::AsRawFd;
use std::{
    io::{ErrorKind, Read, Write},
    net::{IpAddr, Ipv4Addr, Shutdown, TcpListener, TcpStream},
    sync::Arc,
    thread,
    time::Duration,
};

use peritus_network::{
    ConnectionDecision, CredentialLease, CredentialProvider, DestinationDecision,
    DestinationRequest, DnsMode, ManagedProxy, ManagedProxyPreparation, NetworkBounds,
    NetworkObservationKind, NetworkPlan, ProxyCredential, ProxyMode, RedirectChain, RedirectMode,
    RedirectTarget, Resolver, RoutingToken, RuntimeNetworkOptions, ScopedCredential,
};
use peritus_sandbox::{
    DescendantPolicy, DnsName, EnvironmentContract, EnvironmentMode, EnvironmentRequirements,
    FileOperation, FileOperationSet, FileRequirement, FilesystemContract, FilesystemRule,
    HostMatcher, InputPermission, IsolationRequirement, NetworkContract, NetworkHost, NetworkRule,
    NetworkTarget, PathScope, PortRange, ProcessContract, ProcessRequirements, ResizePermission,
    ResourceLimits, RuleEffect, SandboxBinding, SandboxContract, SandboxOperationClass,
    SandboxPath, SandboxRequirements, SecretContract, SecretDelivery, SecretGrant, SecretReference,
    SignalPolicy, TerminalContract, TerminalLimits, TerminalMode, TerminalModes,
    TerminalRequirements, TerminalSignalPermission, Transport, TreeContainment, compile_sandbox,
};
use peritus_types::{
    AcceptanceSpecId, EnvironmentId, Generation, HarnessId, PolicyId, ProcessId, ProviderProfileId,
    ResourceId, ResourceQuantity, RevisionNumber, RevisionTuple, WorkspaceId,
};

#[path = "network/policy.rs"]
mod policy;
#[path = "network/redirects.rs"]
mod redirects;
#[path = "network/support.rs"]
mod support;

use support::*;

#[test]
fn managed_proxy_forwards_http_injects_scoped_credential_and_joins() {
    let _guard = serial_proxy_test();
    let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = upstream.local_addr().unwrap().port();
    let upstream_task = thread::spawn(move || {
        let (mut stream, _) = upstream.accept().unwrap();
        let head = read_head(&mut stream);
        assert!(head.contains("GET /value HTTP/1.1"));
        assert!(head.contains("Authorization: Bearer canary-value"));
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK")
            .unwrap();
    });
    let reference = secret_reference(b"credential-authority");
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
        vec![reference],
    );
    let lease = CredentialLease::new(
        reference,
        HostMatcher::DnsExact(dns("loop.test")),
        Transport::Tcp,
        port,
        "Authorization",
        1,
        u64::MAX,
        plan.digest(),
        plan.owner(),
    )
    .unwrap();
    let credential = Arc::new(ProxyCredential::new(lease, Arc::new(TestCredential)));
    let proxy = ManagedProxy::start_with(
        plan,
        RoutingToken::new([9; 32]),
        Arc::new(LoopbackResolver),
        Some(credential),
    )
    .unwrap();
    let mut client = TcpStream::connect(proxy.endpoint().socket_addr()).unwrap();
    proxy.routing_token().expose_header(|token| {
        write!(
            client,
            "GET http://loop.test:{port}/value HTTP/1.1\r\nHost: loop.test:{port}\r\nProxy-Authorization: Peritus {token}\r\n\r\n"
        )
        .unwrap();
    });
    let response = String::from_utf8(read_to_close(&mut client)).unwrap();
    assert!(response.ends_with("OK"));
    upstream_task.join().unwrap();
    let shutdown = proxy.shutdown().unwrap();
    assert!(shutdown.workers_joined());
    assert_eq!(shutdown.accepted_connections(), 1);
}

#[test]
fn managed_proxy_connect_tunnels_bidirectionally_and_joins_both_relays() {
    let _guard = serial_proxy_test();
    let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = upstream.local_addr().unwrap().port();
    let upstream_task = thread::spawn(move || {
        let (mut stream, _) = upstream.accept().unwrap();
        let mut request = [0_u8; 4];
        stream.read_exact(&mut request).unwrap();
        assert_eq!(&request, b"ping");
        stream.write_all(b"pong").unwrap();
        stream.shutdown(Shutdown::Write).unwrap();
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
        RoutingToken::new([10; 32]),
        Arc::new(LoopbackResolver),
        None,
    )
    .unwrap();
    let mut client = TcpStream::connect(proxy.endpoint().socket_addr()).unwrap();
    client.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    proxy.routing_token().expose_header(|token| {
        write!(
            client,
            "CONNECT loop.test:{port} HTTP/1.1\r\nHost: loop.test:{port}\r\nProxy-Authorization: Peritus {token}\r\n\r\n"
        )
        .unwrap();
    });
    assert!(read_head(&mut client).starts_with("HTTP/1.1 200"));
    client.write_all(b"ping").unwrap();
    client.shutdown(Shutdown::Write).unwrap();
    let response = read_to_close(&mut client);
    assert_eq!(response, b"pong");
    upstream_task.join().unwrap();
    let shutdown = proxy.shutdown().unwrap();
    assert!(shutdown.workers_joined());
    assert_eq!(shutdown.accepted_connections(), 1);
}

#[test]
fn proxy_shutdown_cancels_an_active_connect_and_joins_the_worker() {
    let _guard = serial_proxy_test();
    let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = upstream.local_addr().unwrap().port();
    let upstream_task = thread::spawn(move || {
        let (mut stream, _) = upstream.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut bytes = Vec::new();
        let _ = stream.read_to_end(&mut bytes);
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
        RoutingToken::new([11; 32]),
        Arc::new(LoopbackResolver),
        None,
    )
    .unwrap();
    let mut client = TcpStream::connect(proxy.endpoint().socket_addr()).unwrap();
    client.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    proxy.routing_token().expose_header(|token| {
        write!(
            client,
            "CONNECT loop.test:{port} HTTP/1.1\r\nHost: loop.test:{port}\r\nProxy-Authorization: Peritus {token}\r\n\r\n"
        )
        .unwrap();
    });
    assert!(read_head(&mut client).starts_with("HTTP/1.1 200"));
    let shutdown = proxy.shutdown().unwrap();
    assert!(shutdown.workers_joined());
    assert_eq!(shutdown.accepted_connections(), 1);
    drop(client);
    upstream_task.join().unwrap();
}

#[test]
fn worker_bound_applies_backpressure_and_shutdown_joins_the_active_tunnel() {
    let _guard = serial_proxy_test();
    let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = upstream.local_addr().unwrap().port();
    let upstream_task = thread::spawn(move || {
        let (mut stream, _) = upstream.accept().unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut bytes = Vec::new();
        let _ = stream.read_to_end(&mut bytes);
    });
    let bounds =
        NetworkBounds::new(4, 1, 1_000_000, 4_000_000, 5_000, 30_000, 128, 16_384).unwrap();
    let plan = runtime_plan_with_bounds(
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
        bounds,
    );
    let proxy = ManagedProxy::start_with(
        plan,
        RoutingToken::new([15; 32]),
        Arc::new(LoopbackResolver),
        None,
    )
    .unwrap();
    let mut active = TcpStream::connect(proxy.endpoint().socket_addr()).unwrap();
    active.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    proxy.routing_token().expose_header(|token| {
        let request = format!(
            "CONNECT loop.test:{port} HTTP/1.1\r\nHost: loop.test:{port}\r\nProxy-Authorization: Peritus {token}\r\n\r\n"
        );
        active.write_all(request.as_bytes()).unwrap();
    });
    assert!(read_head(&mut active).starts_with("HTTP/1.1 200"));

    let mut rejected = TcpStream::connect(proxy.endpoint().socket_addr()).unwrap();
    rejected.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    // Worker backpressure is enforced when the connection is accepted, before
    // request parsing. Reading the response directly avoids racing a request
    // write against the intentional close of the rejected socket.
    assert!(read_head(&mut rejected).starts_with("HTTP/1.1 503"));
    let observations = proxy.observations();
    assert!(observations.windows(2).all(|pair| pair[0].sequence() < pair[1].sequence()));
    assert!(proxy.shutdown().unwrap().workers_joined());
    drop(active);
    upstream_task.join().unwrap();
}

#[test]
fn managed_proxy_denies_unlisted_destination_before_resolution_or_connect() {
    let _guard = serial_proxy_test();
    let unused = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = unused.local_addr().unwrap().port();
    let plan = runtime_plan(
        vec![rule(RuleEffect::Allow, HostMatcher::DnsExact(dns("allowed.test")), port)],
        "allowed.test",
        port,
        Vec::new(),
    );
    let proxy =
        ManagedProxy::start_with(plan, RoutingToken::new([12; 32]), Arc::new(PanicResolver), None)
            .unwrap();
    let mut client = TcpStream::connect(proxy.endpoint().socket_addr()).unwrap();
    proxy.routing_token().expose_header(|token| {
        write!(
            client,
            "CONNECT denied.test:{port} HTTP/1.1\r\nHost: denied.test:{port}\r\nProxy-Authorization: Peritus {token}\r\n\r\n"
        )
        .unwrap();
    });
    let response = read_to_close(&mut client);
    assert!(response.is_empty());
    wait_for_closed(&proxy, ConnectionDecision::Denied);
    let shutdown = proxy.shutdown().unwrap();
    assert!(shutdown.workers_joined());
}

#[test]
fn connect_byte_ceiling_stops_the_tunnel_and_reports_a_limited_close() {
    let _guard = serial_proxy_test();
    let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = upstream.local_addr().unwrap().port();
    let upstream_task = thread::spawn(move || {
        let (mut stream, _) = upstream.accept().unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).unwrap();
        assert!(bytes.is_empty());
    });
    let bounds = NetworkBounds::new(4, 2, 40, 80, 5_000, 30_000, 128, 16_384).unwrap();
    let plan = runtime_plan_with_bounds(
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
        bounds,
    );
    let proxy = ManagedProxy::start_with(
        plan,
        RoutingToken::new([14; 32]),
        Arc::new(LoopbackResolver),
        None,
    )
    .unwrap();
    let mut client = TcpStream::connect(proxy.endpoint().socket_addr()).unwrap();
    proxy.routing_token().expose_header(|token| {
        write!(
            client,
            "CONNECT loop.test:{port} HTTP/1.1\r\nHost: loop.test:{port}\r\nProxy-Authorization: Peritus {token}\r\n\r\n"
        )
        .unwrap();
    });
    assert!(read_head(&mut client).starts_with("HTTP/1.1 200"));
    let _ = client.write_all(b"ping");
    let _ = client.shutdown(Shutdown::Write);
    let mut response = Vec::new();
    let _ = client.read_to_end(&mut response);
    upstream_task.join().unwrap();
    wait_for_closed(&proxy, ConnectionDecision::Limited);
    assert!(proxy.shutdown().unwrap().workers_joined());
}

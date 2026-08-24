//! Canonical policy, preparation, listener bridge, and credential-scope tests.

use super::*;

#[test]
fn canonical_matching_denies_and_special_addresses_are_narrowed() {
    let port = 8443;
    let rules = vec![
        rule(RuleEffect::Allow, HostMatcher::DnsSuffix(dns("example.test")), port),
        rule(RuleEffect::Deny, HostMatcher::DnsExact(dns("blocked.example.test")), port),
    ];
    let plan = runtime_plan(rules, "api.example.test", port, Vec::new());
    let allowed = request("api.example.test", port);
    let denied = request("blocked.example.test", port);
    assert_eq!(plan.decide_request(&allowed).unwrap(), DestinationDecision::Allowed);
    assert_eq!(plan.decide_request(&denied).unwrap(), DestinationDecision::DeniedByRule);
    assert!(plan.admit_resolved(&allowed, IpAddr::V4(Ipv4Addr::LOCALHOST)).is_err());

    let with_loopback = runtime_plan(
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
    assert!(
        with_loopback
            .admit_resolved(&request("loop.test", port), IpAddr::V4(Ipv4Addr::LOCALHOST))
            .is_ok()
    );
    assert!(peritus_network::network_decision_no_broader(true, true));
    assert!(!peritus_network::network_decision_no_broader(false, true));
}

#[test]
fn canonical_order_ipv6_and_multi_answer_denial_are_exact() {
    let port = 9443;
    let dns_rule = rule(RuleEffect::Allow, HostMatcher::DnsExact(dns("dual.example.test")), port);
    let v6_rule = rule(
        RuleEffect::Allow,
        HostMatcher::ip_prefix(IpAddr::V6(std::net::Ipv6Addr::LOCALHOST), 128).unwrap(),
        port,
    );
    let first = runtime_plan(
        vec![dns_rule.clone(), v6_rule.clone()],
        "dual.example.test",
        port,
        Vec::new(),
    );
    let second = runtime_plan(vec![v6_rule, dns_rule], "dual.example.test", port, Vec::new());
    assert_eq!(first.digest(), second.digest());
    let request = request("dual.example.test", port);
    assert!(first.admit_resolved(&request, IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)).is_ok());
    assert!(StrictMultiAnswerResolver.resolve(&first, &request).is_err());
}

#[test]
fn inert_proxy_preparation_starts_only_from_the_checked_plan() {
    let port = 8443;
    let rules = vec![rule(RuleEffect::Allow, HostMatcher::DnsExact(dns("api.example.test")), port)];
    let checked = checked_plan(rules, "api.example.test", port, &[]);
    let options = RuntimeNetworkOptions::new(
        DnsMode::ProxySystem,
        RedirectMode::Deny,
        ProxyMode::HttpConnect,
        NetworkBounds::new(2, 1, 8_192, 16_384, 1_000, 5_000, 16, 4_096).unwrap(),
        Vec::new(),
    );
    let token = RoutingToken::new([19; 32]);
    assert_eq!(token.expose_bytes(|bytes| *bytes), [19; 32]);
    let preparation =
        ManagedProxyPreparation::new(options, token, Arc::new(LoopbackResolver), None);
    assert!(!format!("{preparation:?}").contains("191919"));
    let proxy = preparation.prepare(&checked).unwrap();
    assert!(proxy.endpoint().socket_addr().ip().is_loopback());
    assert!(proxy.shutdown().unwrap().workers_joined());
}

#[cfg(unix)]
#[test]
fn inherited_listener_bridge_accepts_inside_namespace_and_connects_from_parent() {
    let _guard = serial_proxy_test();
    let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let upstream_port = upstream.local_addr().unwrap().port();
    let upstream_task = thread::spawn(move || {
        let (mut stream, _) = upstream.accept().unwrap();
        let _ = read_head(&mut stream);
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK")
            .unwrap();
    });
    let rules = vec![
        rule(RuleEffect::Allow, HostMatcher::DnsExact(dns("loop.test")), upstream_port),
        rule(
            RuleEffect::Allow,
            HostMatcher::ip_prefix(IpAddr::V4(Ipv4Addr::LOCALHOST), 32).unwrap(),
            upstream_port,
        ),
    ];
    let checked = checked_plan(rules, "loop.test", upstream_port, &[]);
    let preparation = ManagedProxyPreparation::new(
        RuntimeNetworkOptions::new(
            DnsMode::ProxySystem,
            RedirectMode::Deny,
            ProxyMode::HttpConnect,
            NetworkBounds::new(4, 2, 1_000_000, 2_000_000, 5_000, 30_000, 64, 16_384).unwrap(),
            Vec::new(),
        ),
        RoutingToken::new([20; 32]),
        Arc::new(LoopbackResolver),
        None,
    );
    let mut proxy = preparation.prepare_inherited_listener(&checked).unwrap();
    let channel = proxy.take_listener_channel().unwrap();
    let namespace_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let endpoint = peritus_network::send_inherited_listener(
        u64::try_from(channel.as_raw_fd()).unwrap(),
        &namespace_listener,
    )
    .unwrap();
    drop(namespace_listener);
    for _ in 0..100 {
        if proxy.endpoint() == Some(endpoint) {
            break;
        }
        thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(proxy.endpoint(), Some(endpoint));
    let mut client = TcpStream::connect(endpoint.socket_addr()).unwrap();
    proxy.routing_token().expose_header(|token| {
        write!(
            client,
            "GET http://loop.test:{upstream_port}/value HTTP/1.1\r\nHost: loop.test:{upstream_port}\r\nProxy-Authorization: Peritus {token}\r\n\r\n"
        )
        .unwrap();
    });
    let mut response = String::new();
    client.read_to_string(&mut response).unwrap();
    assert!(response.ends_with("OK"));
    upstream_task.join().unwrap();
    assert!(proxy.shutdown().unwrap().workers_joined());
}

#[test]
fn redirect_and_credential_scope_are_exact_and_bounded() {
    let port = 8080;
    let reference = secret_reference(b"upstream-token");
    let plan = runtime_plan(
        vec![rule(RuleEffect::Allow, HostMatcher::DnsSuffix(dns("example.test")), port)],
        "api.example.test",
        port,
        vec![reference],
    );
    let mut redirects = RedirectChain::new(&plan);
    redirects.follow(RedirectTarget::parse("http://api.example.test:8080/next").unwrap()).unwrap();
    assert!(
        redirects
            .follow(RedirectTarget::parse("http://api.example.test:8080/end").unwrap())
            .is_err()
    );
    let deny_options = RuntimeNetworkOptions::new(
        DnsMode::ProxySystem,
        RedirectMode::Deny,
        ProxyMode::HttpConnect,
        NetworkBounds::new(8, 4, 1_000_000, 4_000_000, 5_000, 30_000, 128, 16_384).unwrap(),
        vec![reference],
    );
    let deny_plan = NetworkPlan::from_checked(
        &checked_plan(
            vec![rule(RuleEffect::Allow, HostMatcher::DnsSuffix(dns("example.test")), port)],
            "api.example.test",
            port,
            &[reference],
        ),
        deny_options,
    )
    .unwrap();
    let relative =
        RedirectTarget::relative(request("api.example.test", port), "/relative").unwrap();
    assert!(RedirectChain::new(&deny_plan).follow(relative).is_err());

    let mut lease = CredentialLease::new(
        reference,
        HostMatcher::DnsExact(dns("api.example.test")),
        Transport::Tcp,
        port,
        "Authorization",
        1,
        10_000,
        plan.digest(),
        plan.owner(),
    )
    .unwrap();
    lease.consume(&request("api.example.test", port), plan.digest(), plan.owner(), 9_999).unwrap();
    assert!(
        lease
            .consume(&request("api.example.test", port), plan.digest(), plan.owner(), 9_999)
            .is_err()
    );
    let routing = RoutingToken::new([7; 32]);
    assert_eq!(format!("{routing:?}"), "RoutingToken([REDACTED])");
    routing.expose_header(|hex| {
        let basic = base64::engine::general_purpose::STANDARD.encode(format!("peritus:{hex}"));
        assert!(routing.verifies_authorization(&format!("Basic {basic}")));
    });
}

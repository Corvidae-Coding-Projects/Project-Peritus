//! Real fresh-network-namespace inherited-listener proxy enforcement.

use super::{native_support::*, support};
use peritus_network::{
    DestinationRequest, DnsMode, ManagedProxyPreparation, NetworkBounds, NetworkPlan, ProxyMode,
    RedirectMode, Resolver, RoutingToken, RuntimeNetworkOptions,
};
use peritus_sandbox_linux::{
    HelperManifest, InheritedHandle, LandlockAccess, LandlockRule, MountPlan, MountPolicy,
    NetworkIsolation, PROXY_LISTENER_LABEL, PROXY_TOKEN_LABEL, TargetCommand,
};
use std::io::{Read, Seek, Write};
use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::sync::Arc;

#[test]
fn bubblewrapped_helper_bridges_only_the_managed_proxy_from_its_fresh_netns() {
    let _guard = native_test_guard();
    if !native_sandbox_available() {
        return;
    }
    let workspace = tempfile::tempdir().expect("workspace");
    for protected in [".git", ".peritus", ".crosslink"] {
        std::fs::create_dir(workspace.path().join(protected)).expect("protected root");
    }
    std::fs::write(workspace.path().join("input.txt"), b"input").expect("input");
    let upstream = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).expect("upstream listener");
    let upstream_port = upstream.local_addr().expect("upstream address").port();
    let upstream_task = std::thread::spawn(move || serve_upstream(&upstream));
    let checked = support::checked_network_plan(workspace.path(), "loop.test", upstream_port);
    let options = RuntimeNetworkOptions::new(
        DnsMode::ProxySystem,
        RedirectMode::Deny,
        ProxyMode::HttpConnect,
        NetworkBounds::new(4, 2, 64 * 1024, 128 * 1024, 5_000, 15_000, 64, 16_384)
            .expect("network bounds"),
        Vec::new(),
    );
    let token = [47_u8; 32];
    let preparation = ManagedProxyPreparation::new(
        options,
        RoutingToken::new(token),
        Arc::new(LoopbackResolver),
        None,
    );
    let mut proxy =
        preparation.prepare_inherited_listener(&checked).expect("inherited listener proxy");
    let channel = proxy.take_listener_channel().expect("listener channel");
    let channel_flags = make_inheritable(&channel);
    let mut token_file = tempfile::tempfile().expect("routing token file");
    token_file.write_all(&token).expect("routing token");
    token_file.seek(std::io::SeekFrom::Start(0)).expect("rewind routing token");
    let token_flags = make_inheritable(&token_file);
    let inherited = vec![
        InheritedHandle::new(
            u64::try_from(channel.as_raw_fd()).expect("channel descriptor"),
            PROXY_LISTENER_LABEL.to_owned(),
        )
        .expect("channel handle"),
        InheritedHandle::new(
            u64::try_from(token_file.as_raw_fd()).expect("token descriptor"),
            PROXY_TOKEN_LABEL.to_owned(),
        )
        .expect("token handle"),
    ];
    let cgroup_leaf = workspace.path().join("peritus-test-cgroup");
    std::fs::create_dir_all(&cgroup_leaf).expect("cgroup stand-in");
    std::fs::write(cgroup_leaf.join("cgroup.procs"), b"").expect("membership stand-in");
    let manifest = HelperManifest::new(
        digest(51),
        digest(52),
        digest(53),
        digest(54),
        TargetCommand::new(
            "/usr/bin/curl".to_owned(),
            vec![
                "--silent".to_owned(),
                "--show-error".to_owned(),
                "--fail".to_owned(),
                "--max-time".to_owned(),
                "5".to_owned(),
                format!("http://loop.test:{upstream_port}/value"),
            ],
        )
        .expect("curl target"),
        workspace.path().to_path_buf(),
        cgroup_leaf,
        false,
        Vec::new(),
        vec![
            LandlockRule::new(PathBuf::from("/"), LandlockAccess::host_read_only()).expect("rule"),
        ],
        support::resource_plan(),
        NetworkIsolation::ManagedProxy,
        inherited,
    )
    .expect("proxy manifest");
    let mounts = MountPlan::project(
        &support::checked_plan(workspace.path()),
        &MountPolicy::new(workspace.path(), Vec::new()).expect("mount policy"),
    )
    .expect("mount plan");
    let output = run_bubblewrapped(&mounts, &manifest);
    restore_descriptor_flags(&channel, channel_flags);
    restore_descriptor_flags(&token_file, token_flags);
    assert!(output.status.success(), "helper stderr: {}", output.stderr);
    assert_eq!(output.target_stdout, b"OK");
    assert!(proxy.endpoint().is_some());
    assert!(proxy.shutdown().expect("proxy shutdown").workers_joined());
    upstream_task.join().expect("upstream task");
}

fn serve_upstream(upstream: &TcpListener) {
    let (mut stream, _) = upstream.accept().expect("upstream accept");
    let mut request = Vec::new();
    let mut byte = [0_u8; 1];
    while !request.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).expect("request byte");
        request.push(byte[0]);
    }
    assert!(String::from_utf8_lossy(&request).contains("GET /value HTTP/1.1"));
    stream
        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nOK")
        .expect("upstream response");
}

struct LoopbackResolver;

impl Resolver for LoopbackResolver {
    fn resolve(
        &self,
        plan: &NetworkPlan,
        request: &DestinationRequest,
    ) -> Result<Vec<peritus_network::ResolvedDestination>, peritus_network::NetworkError> {
        Ok(vec![plan.admit_resolved(request, IpAddr::V4(Ipv4Addr::LOCALHOST))?])
    }
}

//! Shared deterministic managed-network test fixtures.

#![allow(missing_docs, reason = "integration-test support module")]

use super::*;

pub struct LoopbackResolver;

impl Resolver for LoopbackResolver {
    fn resolve(
        &self,
        plan: &NetworkPlan,
        request: &DestinationRequest,
    ) -> Result<Vec<peritus_network::ResolvedDestination>, peritus_network::NetworkError> {
        Ok(vec![plan.admit_resolved(request, IpAddr::V4(Ipv4Addr::LOCALHOST))?])
    }
}

pub struct PanicResolver;

impl Resolver for PanicResolver {
    fn resolve(
        &self,
        _plan: &NetworkPlan,
        _request: &DestinationRequest,
    ) -> Result<Vec<peritus_network::ResolvedDestination>, peritus_network::NetworkError> {
        panic!("denied destination reached resolver")
    }
}

pub struct StrictMultiAnswerResolver;

impl Resolver for StrictMultiAnswerResolver {
    fn resolve(
        &self,
        plan: &NetworkPlan,
        request: &DestinationRequest,
    ) -> Result<Vec<peritus_network::ResolvedDestination>, peritus_network::NetworkError> {
        [IpAddr::V6(std::net::Ipv6Addr::LOCALHOST), IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))]
            .into_iter()
            .map(|address| plan.admit_resolved(request, address))
            .collect()
    }
}

pub struct TestCredential;

impl CredentialProvider for TestCredential {
    fn acquire(
        &self,
        _reference: SecretReference,
    ) -> Result<ScopedCredential, peritus_network::NetworkError> {
        ScopedCredential::new(b"Bearer canary-value".to_vec())
    }
}

pub fn runtime_plan(
    rules: Vec<NetworkRule>,
    required_host: &str,
    required_port: u16,
    credentials: Vec<SecretReference>,
) -> NetworkPlan {
    runtime_plan_with_bounds(
        rules,
        required_host,
        required_port,
        credentials,
        NetworkBounds::new(8, 4, 1_000_000, 4_000_000, 5_000, 30_000, 128, 16_384).unwrap(),
    )
}

pub fn runtime_plan_with_bounds(
    rules: Vec<NetworkRule>,
    required_host: &str,
    required_port: u16,
    credentials: Vec<SecretReference>,
    bounds: NetworkBounds,
) -> NetworkPlan {
    let checked = checked_plan(rules, required_host, required_port, &credentials);
    NetworkPlan::from_checked(
        &checked,
        RuntimeNetworkOptions::new(
            DnsMode::ProxySystem,
            RedirectMode::Follow { maximum: 1 },
            ProxyMode::HttpConnect,
            bounds,
            credentials,
        ),
    )
    .unwrap()
}

pub fn wait_for_closed(proxy: &ManagedProxy, decision: ConnectionDecision) {
    for _ in 0..100 {
        if proxy.observations().iter().any(|observation| {
            observation.kind() == NetworkObservationKind::Closed
                && observation.decision() == decision
        }) {
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    panic!("proxy did not publish the expected closed observation");
}

pub fn read_to_close(stream: &mut TcpStream) -> Vec<u8> {
    let mut bytes = Vec::new();
    if let Err(error) = stream.read_to_end(&mut bytes) {
        assert_eq!(error.kind(), ErrorKind::ConnectionReset);
    }
    bytes
}

pub fn checked_plan(
    rules: Vec<NetworkRule>,
    required_host: &str,
    required_port: u16,
    credentials: &[SecretReference],
) -> peritus_sandbox::CheckedSandboxPlan {
    let workspace_ops = FileOperationSet::from_operations([FileOperation::Read]);
    let filesystem = FilesystemContract::new(vec![
        FilesystemRule::new(
            RuleEffect::Allow,
            SandboxPath::new("/workspace").unwrap(),
            PathScope::Descendants,
            workspace_ops,
        )
        .unwrap(),
        FilesystemRule::new(
            RuleEffect::Allow,
            SandboxPath::new("/bin/tool").unwrap(),
            PathScope::Exact,
            FileOperationSet::from_operations([FileOperation::Execute]),
        )
        .unwrap(),
    ])
    .unwrap();
    let process = ProcessContract::new(
        vec![SandboxPath::new("/bin/tool").unwrap()],
        DescendantPolicy::Bounded(1),
        SignalPolicy::GracefulAndForced,
        TreeContainment::Required,
        2,
    )
    .unwrap();
    let environment = EnvironmentContract::new(EnvironmentMode::Cleared, Vec::new()).unwrap();
    let grants: Vec<_> = credentials
        .iter()
        .map(|reference| {
            SecretGrant::new(
                *reference,
                SecretDelivery::Environment(
                    peritus_sandbox::EnvironmentName::new("TOKEN").unwrap(),
                ),
            )
        })
        .collect();
    let secrets = SecretContract::new(grants.clone()).unwrap();
    let terminal = TerminalContract::new(
        TerminalModes::from_modes([TerminalMode::Pipes]),
        InputPermission::Allowed,
        ResizePermission::Denied,
        TerminalSignalPermission::Allowed,
        TerminalLimits::new(None, ResourceQuantity::new(1_000_000), ResourceQuantity::new(10_000))
            .unwrap(),
    )
    .unwrap();
    let contract = SandboxContract::new(
        filesystem,
        process,
        environment,
        NetworkContract::new(rules).unwrap(),
        secrets,
        limits(10_000_000),
        terminal,
    );
    let requirements = SandboxRequirements::new(
        vec![FileRequirement::new(
            SandboxPath::new("/workspace/input").unwrap(),
            FileOperation::Read,
        )],
        ProcessRequirements::new(SandboxPath::new("/bin/tool").unwrap(), 0, true),
        EnvironmentRequirements::new(Vec::new(), Vec::new()).unwrap(),
        vec![
            NetworkTarget::new(NetworkHost::Dns(dns(required_host)), Transport::Tcp, required_port)
                .unwrap(),
        ],
        grants,
        limits(1_000_000),
        TerminalRequirements::new(
            TerminalMode::Pipes,
            InputPermission::Allowed,
            ResizePermission::Denied,
            TerminalSignalPermission::Allowed,
            None,
            ResourceQuantity::new(100_000),
            ResourceQuantity::new(10_000),
        )
        .unwrap(),
    )
    .unwrap();
    compile_sandbox(
        binding(),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        contract,
        requirements,
    )
    .unwrap()
}

fn binding() -> SandboxBinding {
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new([1; 16]).unwrap(),
        HarnessId::new([2; 16]).unwrap(),
        WorkspaceId::new([3; 16]).unwrap(),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new([4; 16]).unwrap(),
        ProviderProfileId::new([5; 16]).unwrap(),
    );
    SandboxBinding::new(
        ProcessId::new([6; 16]).unwrap(),
        ResourceId::new([7; 16]).unwrap(),
        EnvironmentId::new([8; 16]).unwrap(),
        revision,
    )
}

fn limits(value: u64) -> ResourceLimits {
    ResourceLimits::new(
        ResourceQuantity::new(value),
        ResourceQuantity::new(value),
        ResourceQuantity::new(value),
        ResourceQuantity::new(value),
        ResourceQuantity::new(value),
        ResourceQuantity::new(value),
        ResourceQuantity::new(value),
        ResourceQuantity::new(value),
    )
    .unwrap()
}

pub fn dns(value: &str) -> DnsName {
    DnsName::new(value).unwrap()
}

pub fn rule(effect: RuleEffect, host: HostMatcher, port: u16) -> NetworkRule {
    NetworkRule::new(effect, host, Transport::Tcp, PortRange::new(port, port).unwrap())
}

pub fn request(host: &str, port: u16) -> DestinationRequest {
    DestinationRequest::new(NetworkHost::Dns(dns(host)), Transport::Tcp, port).unwrap()
}

pub fn secret_reference(value: &[u8]) -> SecretReference {
    SecretReference::new(ResourceId::new([33; 16]).unwrap(), peritus_codec::sha256(value))
}

pub fn read_head(stream: &mut TcpStream) -> String {
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    let mut bytes = Vec::new();
    let mut byte = [0; 1];
    while !bytes.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).unwrap();
        bytes.push(byte[0]);
    }
    String::from_utf8(bytes).unwrap()
}

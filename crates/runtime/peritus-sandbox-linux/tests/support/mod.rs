#![allow(dead_code, reason = "shared integration-test fixtures")]

use peritus_sandbox::{
    CheckedSandboxPlan, DescendantPolicy, DnsName, EnvironmentContract, EnvironmentMode,
    EnvironmentRequirements, FileOperation, FileOperationSet, FileRequirement, FilesystemContract,
    FilesystemRule, HostMatcher, InputPermission, IsolationRequirement, NetworkContract,
    NetworkHost, NetworkRule, NetworkTarget, PathScope, PortRange, ProcessContract,
    ProcessRequirements, ResizePermission, ResourceLimits, RuleEffect, SandboxBinding,
    SandboxContract, SandboxOperationClass, SandboxPath, SandboxRequirements, SecretContract,
    SignalPolicy, TerminalContract, TerminalLimits, TerminalMode, TerminalModes,
    TerminalRequirements, TerminalSignalPermission, Transport, TreeContainment, compile_sandbox,
};
use peritus_types::{
    AcceptanceSpecId, EnvironmentId, Generation, HarnessId, PolicyId, ProcessId, ProviderProfileId,
    ResourceId, ResourceQuantity, RevisionNumber, RevisionTuple, WorkspaceId,
};
use std::path::Path;

pub fn limits(value: u64) -> ResourceLimits {
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
    .expect("nonzero fixture resource limits")
}

pub fn resource_plan() -> peritus_sandbox_linux::ResourcePlan {
    let limits = ResourceLimits::new(
        ResourceQuantity::new(10_000),
        ResourceQuantity::new(5_000),
        ResourceQuantity::new(1024 * 1024 * 1024),
        ResourceQuantity::new(16 * 1024 * 1024),
        ResourceQuantity::new(1024 * 1024),
        ResourceQuantity::new(256),
        ResourceQuantity::new(1_024),
        ResourceQuantity::new(8),
    )
    .expect("valid helper resource limits");
    peritus_sandbox_linux::ResourcePlan::from_limits(&limits)
}

pub fn checked_plan(workspace: &Path) -> CheckedSandboxPlan {
    checked_plan_with_network(workspace, NetworkContract::deny_all(), Vec::new())
}

pub fn checked_network_plan(workspace: &Path, host: &str, port: u16) -> CheckedSandboxPlan {
    let name = DnsName::new(host).expect("network host");
    let range = PortRange::new(port, port).expect("network port");
    let contract = NetworkContract::new(vec![
        NetworkRule::new(
            RuleEffect::Allow,
            HostMatcher::DnsExact(name.clone()),
            Transport::Tcp,
            range,
        ),
        NetworkRule::new(
            RuleEffect::Allow,
            HostMatcher::ip_prefix(std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST), 32)
                .expect("loopback prefix"),
            Transport::Tcp,
            range,
        ),
    ])
    .expect("network contract");
    let requirements = vec![
        NetworkTarget::new(NetworkHost::Dns(name), Transport::Tcp, port)
            .expect("network requirement"),
    ];
    checked_plan_with_network(workspace, contract, requirements)
}

fn checked_plan_with_network(
    workspace: &Path,
    network: NetworkContract,
    network_requirements: Vec<NetworkTarget>,
) -> CheckedSandboxPlan {
    let root = SandboxPath::new(workspace.to_string_lossy().into_owned()).expect("workspace path");
    let git = SandboxPath::new(workspace.join(".git").to_string_lossy().into_owned())
        .expect("metadata path");
    let program = SandboxPath::new("/usr/bin/true").expect("program path");
    let all = FileOperationSet::from_operations([
        FileOperation::Discover,
        FileOperation::Metadata,
        FileOperation::Read,
        FileOperation::Execute,
        FileOperation::Create,
        FileOperation::Write,
        FileOperation::Remove,
    ]);
    let filesystem = FilesystemContract::new(vec![
        FilesystemRule::new(RuleEffect::Allow, root, PathScope::Descendants, all)
            .expect("workspace allow"),
        FilesystemRule::new(RuleEffect::Deny, git, PathScope::Descendants, all)
            .expect("metadata deny"),
        FilesystemRule::new(
            RuleEffect::Allow,
            program.clone(),
            PathScope::Exact,
            FileOperationSet::from_operations([
                FileOperation::Discover,
                FileOperation::Metadata,
                FileOperation::Read,
                FileOperation::Execute,
            ]),
        )
        .expect("program allow"),
    ])
    .expect("filesystem contract");
    let process = ProcessContract::new(
        vec![program.clone()],
        DescendantPolicy::Denied,
        SignalPolicy::GracefulAndForced,
        TreeContainment::Required,
        1,
    )
    .expect("process contract");
    let terminal_limits =
        TerminalLimits::new(None, ResourceQuantity::new(32), ResourceQuantity::new(1024 * 1024))
            .expect("terminal limits");
    let terminal = TerminalContract::new(
        TerminalModes::from_modes([TerminalMode::Pipes]),
        InputPermission::Denied,
        ResizePermission::Denied,
        TerminalSignalPermission::Denied,
        terminal_limits,
    )
    .expect("terminal contract");
    let contract = SandboxContract::new(
        filesystem,
        process,
        EnvironmentContract::new(EnvironmentMode::Cleared, Vec::new()).expect("environment"),
        network,
        SecretContract::deny_all(),
        limits(2_000_000_000),
        terminal,
    );
    let input = workspace.join("input.txt");
    let requirements = SandboxRequirements::new(
        vec![FileRequirement::new(
            SandboxPath::new(input.to_string_lossy().into_owned()).expect("input path"),
            FileOperation::Read,
        )],
        ProcessRequirements::new(program, 0, false),
        EnvironmentRequirements::new(Vec::new(), Vec::new()).expect("environment requirements"),
        network_requirements,
        Vec::new(),
        limits(1_000_000_000),
        TerminalRequirements::new(
            TerminalMode::Pipes,
            InputPermission::Denied,
            ResizePermission::Denied,
            TerminalSignalPermission::Denied,
            None,
            ResourceQuantity::new(16),
            ResourceQuantity::new(1024),
        )
        .expect("terminal requirements"),
    )
    .expect("sandbox requirements");
    compile_sandbox(
        binding(),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        contract,
        requirements,
    )
    .expect("checked plan")
}

fn binding() -> SandboxBinding {
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new([1; 16]).expect("acceptance ID"),
        HarnessId::new([2; 16]).expect("harness ID"),
        WorkspaceId::new([3; 16]).expect("workspace ID"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new([4; 16]).expect("policy ID"),
        ProviderProfileId::new([5; 16]).expect("provider ID"),
    );
    SandboxBinding::new(
        ProcessId::new([6; 16]).expect("process ID"),
        ResourceId::new([7; 16]).expect("resource ID"),
        EnvironmentId::new([8; 16]).expect("environment ID"),
        revision,
    )
}

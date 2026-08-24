use peritus_sandbox::{
    CheckedSandboxPlan, DescendantPolicy, DnsName, EnvironmentContract, EnvironmentMode,
    EnvironmentName, EnvironmentRequirements, FileOperation, FileOperationSet, FileRequirement,
    FilesystemContract, FilesystemRule, HostMatcher, InputPermission, IsolationRequirement,
    NetworkContract, NetworkHost, NetworkRule, NetworkTarget, PathScope, PortRange,
    ProcessContract, ProcessRequirements, ResizePermission, ResourceLimits, RuleEffect,
    SandboxBinding, SandboxContract, SandboxOperationClass, SandboxPath, SandboxRequirements,
    SecretContract, SecretDelivery, SecretGrant, SecretReference, SignalPolicy, TerminalContract,
    TerminalLimits, TerminalMode, TerminalModes, TerminalRequirements, TerminalSignalPermission,
    TerminalSize, Transport, TreeContainment, compile_sandbox,
};
use peritus_types::{
    AcceptanceSpecId, EnvironmentId, Generation, HarnessId, PolicyId, ProcessId, ProviderProfileId,
    ResourceId, ResourceQuantity, RevisionNumber, RevisionTuple, Sha256Digest, WorkspaceId,
};

pub fn binding(seed: u8) -> SandboxBinding {
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new([seed; 16]).unwrap(),
        HarnessId::new([seed.wrapping_add(1); 16]).unwrap(),
        WorkspaceId::new([seed.wrapping_add(2); 16]).unwrap(),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new([seed.wrapping_add(3); 16]).unwrap(),
        ProviderProfileId::new([seed.wrapping_add(4); 16]).unwrap(),
    );
    SandboxBinding::new(
        ProcessId::new([seed.wrapping_add(5); 16]).unwrap(),
        ResourceId::new([seed.wrapping_add(6); 16]).unwrap(),
        EnvironmentId::new([seed.wrapping_add(7); 16]).unwrap(),
        revision,
    )
}

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
    .unwrap()
}

pub fn allowed_secret() -> SecretGrant {
    SecretGrant::new(
        SecretReference::new(ResourceId::new([41; 16]).unwrap(), Sha256Digest::new([42; 32])),
        SecretDelivery::Environment(EnvironmentName::new("TOKEN").unwrap()),
    )
}

pub fn contract_and_requirements(reverse: bool) -> (SandboxContract, SandboxRequirements) {
    let workspace_operations = FileOperationSet::from_operations([
        FileOperation::Read,
        FileOperation::Create,
        FileOperation::Write,
    ]);
    let mut filesystem_rules = vec![
        FilesystemRule::new(
            RuleEffect::Allow,
            SandboxPath::new("/workspace").unwrap(),
            PathScope::Descendants,
            workspace_operations,
        )
        .unwrap(),
        FilesystemRule::new(
            RuleEffect::Allow,
            SandboxPath::new("/bin/tool").unwrap(),
            PathScope::Exact,
            FileOperationSet::from_operations([FileOperation::Execute]),
        )
        .unwrap(),
    ];
    if reverse {
        filesystem_rules.reverse();
    }
    let filesystem = FilesystemContract::new(filesystem_rules).unwrap();
    let process = ProcessContract::new(
        vec![SandboxPath::new("/bin/tool").unwrap()],
        DescendantPolicy::Bounded(2),
        SignalPolicy::GracefulAndForced,
        TreeContainment::Required,
        3,
    )
    .unwrap();
    let environment = EnvironmentContract::new(
        EnvironmentMode::AllowListed(vec![EnvironmentName::new("PATH").unwrap()]),
        vec![EnvironmentName::new("MODE").unwrap()],
    )
    .unwrap();
    let network_rule = NetworkRule::new(
        RuleEffect::Allow,
        HostMatcher::DnsExact(DnsName::new("example.test").unwrap()),
        Transport::Tcp,
        PortRange::new(443, 443).unwrap(),
    );
    let network = NetworkContract::new(vec![network_rule]).unwrap();
    let secrets = SecretContract::new(vec![allowed_secret()]).unwrap();
    let terminal = terminal_contract();
    let contract = SandboxContract::new(
        filesystem,
        process,
        environment,
        network,
        secrets,
        limits(100),
        terminal,
    );

    let files = vec![FileRequirement::new(
        SandboxPath::new("/workspace/input.rs").unwrap(),
        FileOperation::Read,
    )];
    let process_requirements =
        ProcessRequirements::new(SandboxPath::new("/bin/tool").unwrap(), 1, true);
    let environment_requirements = EnvironmentRequirements::new(
        vec![EnvironmentName::new("PATH").unwrap()],
        vec![EnvironmentName::new("MODE").unwrap()],
    )
    .unwrap();
    let target = NetworkTarget::new(
        NetworkHost::Dns(DnsName::new("example.test").unwrap()),
        Transport::Tcp,
        443,
    )
    .unwrap();
    let terminal_requirements = terminal_requirements();
    let requirements = SandboxRequirements::new(
        files,
        process_requirements,
        environment_requirements,
        vec![target],
        vec![allowed_secret()],
        limits(50),
        terminal_requirements,
    )
    .unwrap();
    (contract, requirements)
}

fn terminal_contract() -> TerminalContract {
    TerminalContract::new(
        TerminalModes::from_modes([TerminalMode::Pty]),
        InputPermission::Allowed,
        ResizePermission::Denied,
        TerminalSignalPermission::Allowed,
        TerminalLimits::new(
            Some(TerminalSize::new(200, 100).unwrap()),
            ResourceQuantity::new(100),
            ResourceQuantity::new(100),
        )
        .unwrap(),
    )
    .unwrap()
}

fn terminal_requirements() -> TerminalRequirements {
    TerminalRequirements::new(
        TerminalMode::Pty,
        InputPermission::Allowed,
        ResizePermission::Denied,
        TerminalSignalPermission::Allowed,
        Some(TerminalSize::new(120, 40).unwrap()),
        ResourceQuantity::new(50),
        ResourceQuantity::new(50),
    )
    .unwrap()
}

pub fn checked_plan() -> CheckedSandboxPlan {
    let (contract, requirements) = contract_and_requirements(false);
    compile_sandbox(
        binding(1),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        contract,
        requirements,
    )
    .unwrap()
}

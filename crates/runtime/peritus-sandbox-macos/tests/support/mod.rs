use peritus_sandbox::{
    CheckedSandboxPlan, DescendantPolicy, EnvironmentContract, EnvironmentMode,
    EnvironmentRequirements, FileOperation, FileOperationSet, FileRequirement, FilesystemContract,
    FilesystemRule, InputPermission, IsolationRequirement, NetworkContract, NetworkRule, PathScope,
    ProcessContract, ProcessRequirements, ResizePermission, ResourceLimits, RuleEffect,
    SandboxBinding, SandboxContract, SandboxOperationClass, SandboxPath, SandboxRequirements,
    SecretContract, SignalPolicy, TerminalContract, TerminalLimits, TerminalMode, TerminalModes,
    TerminalRequirements, TerminalSignalPermission, Transport, TreeContainment, compile_sandbox,
};
use peritus_types::{
    AcceptanceSpecId, EnvironmentId, Generation, HarnessId, PolicyId, ProcessId, ProviderProfileId,
    ResourceId, ResourceQuantity, RevisionNumber, RevisionTuple, WorkspaceId,
};

pub fn checked_plan(extra_rules: Vec<FilesystemRule>) -> CheckedSandboxPlan {
    checked_plan_with_network(extra_rules, NetworkContract::deny_all())
}

pub fn checked_plan_with_network(
    mut extra_rules: Vec<FilesystemRule>,
    network: NetworkContract,
) -> CheckedSandboxPlan {
    extra_rules.push(
        FilesystemRule::new(
            RuleEffect::Allow,
            SandboxPath::new("/bin/tool").unwrap(),
            PathScope::Exact,
            FileOperationSet::from_operations([FileOperation::Execute]),
        )
        .unwrap(),
    );
    extra_rules.push(
        FilesystemRule::new(
            RuleEffect::Allow,
            SandboxPath::new("/workspace").unwrap(),
            PathScope::Descendants,
            FileOperationSet::from_operations([
                FileOperation::Discover,
                FileOperation::Metadata,
                FileOperation::Read,
                FileOperation::Create,
                FileOperation::Write,
                FileOperation::Remove,
            ]),
        )
        .unwrap(),
    );
    let filesystem = FilesystemContract::new(extra_rules).unwrap();
    let process = ProcessContract::new(
        vec![SandboxPath::new("/bin/tool").unwrap()],
        DescendantPolicy::Bounded(2),
        SignalPolicy::Denied,
        TreeContainment::Required,
        3,
    )
    .unwrap();
    let resources = limits(100);
    let terminal_limits =
        TerminalLimits::new(None, ResourceQuantity::new(32), ResourceQuantity::new(100)).unwrap();
    let terminal = TerminalContract::new(
        TerminalModes::from_modes([TerminalMode::Pipes]),
        InputPermission::Denied,
        ResizePermission::Denied,
        TerminalSignalPermission::Denied,
        terminal_limits,
    )
    .unwrap();
    let contract = SandboxContract::new(
        filesystem,
        process,
        EnvironmentContract::new(EnvironmentMode::Cleared, Vec::new()).unwrap(),
        network,
        SecretContract::deny_all(),
        resources,
        terminal,
    );
    let requirements = SandboxRequirements::new(
        vec![
            FileRequirement::new(SandboxPath::new("/bin/tool").unwrap(), FileOperation::Execute),
            FileRequirement::new(
                SandboxPath::new("/workspace/input").unwrap(),
                FileOperation::Read,
            ),
        ],
        ProcessRequirements::new(SandboxPath::new("/bin/tool").unwrap(), 1, false),
        EnvironmentRequirements::new(Vec::new(), Vec::new()).unwrap(),
        Vec::new(),
        Vec::new(),
        resources,
        TerminalRequirements::new(
            TerminalMode::Pipes,
            InputPermission::Denied,
            ResizePermission::Denied,
            TerminalSignalPermission::Denied,
            None,
            ResourceQuantity::new(16),
            ResourceQuantity::new(50),
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

pub fn binding() -> SandboxBinding {
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

pub fn tcp_rule(effect: RuleEffect) -> NetworkRule {
    NetworkRule::new(
        effect,
        peritus_sandbox::HostMatcher::DnsExact(
            peritus_sandbox::DnsName::new("example.test").unwrap(),
        ),
        Transport::Tcp,
        peritus_sandbox::PortRange::new(443, 443).unwrap(),
    )
}

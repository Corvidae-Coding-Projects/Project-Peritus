//! Contract validation, evaluation, and canonical plan identity tests.

mod support;

use peritus_sandbox::{
    DnsName, EnvironmentName, EnvironmentRequirements, FileDecision, FileOperation,
    FileOperationSet, FilesystemContract, FilesystemRule, HostMatcher, InputPermission,
    IsolationRequirement, NetworkContract, NetworkDecision, NetworkHost, NetworkRule,
    NetworkTarget, PathScope, PortRange, ResizePermission, RuleEffect, SandboxContract,
    SandboxErrorKind, SandboxOperationClass, SandboxPath, SandboxRequirements, TerminalContract,
    TerminalLimits, TerminalMode, TerminalModes, TerminalRequirements, TerminalSignalPermission,
    TerminalSize, Transport, compile_sandbox,
};
use peritus_types::ResourceQuantity;
use std::net::{IpAddr, Ipv4Addr};

#[test]
fn domain_values_reject_ambiguous_or_nonportable_input() {
    assert!(SandboxPath::new("relative").is_err());
    assert!(SandboxPath::new("/workspace/../secret").is_err());
    assert_eq!(SandboxPath::new("c:/Mixed/Case").unwrap().as_str(), "C:/Mixed/Case");
    assert!(EnvironmentName::new("BAD-NAME").is_err());
    assert_eq!(EnvironmentName::new("path").unwrap().as_str(), "PATH");
    assert!(DnsName::new("bad..name").is_err());
    assert!(PortRange::new(0, 80).is_err());
}

#[test]
fn filesystem_and_network_are_default_deny_with_deny_precedence() {
    let path = SandboxPath::new("/workspace/private/key").unwrap();
    let operations = FileOperationSet::from_operations([FileOperation::Read]);
    let filesystem = FilesystemContract::new(vec![
        FilesystemRule::new(
            RuleEffect::Allow,
            SandboxPath::new("/workspace").unwrap(),
            PathScope::Descendants,
            operations,
        )
        .unwrap(),
        FilesystemRule::new(
            RuleEffect::Deny,
            SandboxPath::new("/workspace/private").unwrap(),
            PathScope::Descendants,
            operations,
        )
        .unwrap(),
    ])
    .unwrap();
    assert_eq!(filesystem.decide(&path, FileOperation::Read), FileDecision::DeniedByRule);
    assert_eq!(
        filesystem.decide(&SandboxPath::new("/outside").unwrap(), FileOperation::Read),
        FileDecision::DeniedByDefault
    );

    let target = NetworkTarget::new(
        NetworkHost::Dns(DnsName::new("api.example.test").unwrap()),
        Transport::Tcp,
        443,
    )
    .unwrap();
    let network = NetworkContract::new(vec![
        NetworkRule::new(
            RuleEffect::Allow,
            HostMatcher::DnsSuffix(DnsName::new("example.test").unwrap()),
            Transport::Tcp,
            PortRange::new(443, 443).unwrap(),
        ),
        NetworkRule::new(
            RuleEffect::Deny,
            HostMatcher::DnsExact(DnsName::new("api.example.test").unwrap()),
            Transport::Tcp,
            PortRange::new(443, 443).unwrap(),
        ),
    ])
    .unwrap();
    assert_eq!(network.decide(&target), NetworkDecision::DeniedByRule);
}

#[test]
fn ip_prefixes_clear_host_bits_before_identity_and_evaluation() {
    let matcher = HostMatcher::ip_prefix(IpAddr::V4(Ipv4Addr::new(10, 20, 30, 99)), 24).unwrap();
    assert_eq!(
        matcher,
        HostMatcher::IpPrefix {
            address: IpAddr::V4(Ipv4Addr::new(10, 20, 30, 0)),
            prefix_length: 24,
        }
    );
    let contract = NetworkContract::new(vec![NetworkRule::new(
        RuleEffect::Allow,
        matcher,
        Transport::Tcp,
        PortRange::new(80, 80).unwrap(),
    )])
    .unwrap();
    let target = NetworkTarget::new(
        NetworkHost::Ip(IpAddr::V4(Ipv4Addr::new(10, 20, 30, 7))),
        Transport::Tcp,
        80,
    )
    .unwrap();
    assert_eq!(contract.decide(&target), NetworkDecision::Allowed);
}

#[test]
fn canonical_plan_digest_ignores_input_permutation_and_binds_target() {
    let (right_contract, right_requirements) = support::contract_and_requirements(true);
    let left = support::checked_plan();
    let right = compile_sandbox(
        support::binding(1),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        right_contract,
        right_requirements,
    )
    .unwrap();
    assert_eq!(left.digest(), right.digest());

    let (contract, requirements) = support::contract_and_requirements(false);
    let rebound = compile_sandbox(
        support::binding(2),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        contract,
        requirements,
    )
    .unwrap();
    assert_ne!(left.digest(), rebound.digest());
}

#[test]
fn compile_rejects_operation_class_or_requirement_broadening() {
    let (contract, requirements) = support::contract_and_requirements(false);
    let error = compile_sandbox(
        support::binding(1),
        IsolationRequirement::Restricted,
        SandboxOperationClass::RawEffect,
        contract,
        requirements,
    )
    .unwrap_err();
    assert_eq!(error.kind(), SandboxErrorKind::RequirementDenied);

    let (contract, mut requirements) = support::contract_and_requirements(false);
    let denied_environment =
        EnvironmentRequirements::new(vec![EnvironmentName::new("HOME").unwrap()], Vec::new())
            .unwrap();
    requirements = SandboxRequirements::new(
        requirements.files().to_vec(),
        requirements.process().clone(),
        denied_environment,
        requirements.network().to_vec(),
        requirements.secrets().to_vec(),
        *requirements.resources(),
        requirements.terminal(),
    )
    .unwrap();
    assert!(
        compile_sandbox(
            support::binding(1),
            IsolationRequirement::Restricted,
            SandboxOperationClass::Execution,
            contract,
            requirements,
        )
        .is_err()
    );
}

#[test]
fn terminal_bounds_validate_and_reject_each_one_over_requirement() {
    assert!(
        TerminalLimits::new(None, ResourceQuantity::new(4), ResourceQuantity::new(1),).is_err()
    );
    let pipe_limits =
        TerminalLimits::new(None, ResourceQuantity::new(10), ResourceQuantity::new(10)).unwrap();
    assert!(
        TerminalContract::new(
            TerminalModes::from_modes([TerminalMode::Pipes]),
            InputPermission::Denied,
            ResizePermission::Allowed,
            TerminalSignalPermission::Denied,
            pipe_limits,
        )
        .is_err()
    );
    assert!(
        TerminalRequirements::new(
            TerminalMode::Pipes,
            InputPermission::Denied,
            ResizePermission::Denied,
            TerminalSignalPermission::Denied,
            Some(TerminalSize::new(80, 24).unwrap()),
            ResourceQuantity::new(10),
            ResourceQuantity::new(10),
        )
        .is_err()
    );

    let (contract, requirements) = support::contract_and_requirements(false);
    let excessive = [
        TerminalRequirements::new(
            TerminalMode::Pty,
            InputPermission::Allowed,
            ResizePermission::Denied,
            TerminalSignalPermission::Allowed,
            Some(TerminalSize::new(201, 40).unwrap()),
            ResourceQuantity::new(50),
            ResourceQuantity::new(50),
        )
        .unwrap(),
        TerminalRequirements::new(
            TerminalMode::Pty,
            InputPermission::Allowed,
            ResizePermission::Denied,
            TerminalSignalPermission::Allowed,
            Some(TerminalSize::new(120, 40).unwrap()),
            ResourceQuantity::new(101),
            ResourceQuantity::new(50),
        )
        .unwrap(),
        TerminalRequirements::new(
            TerminalMode::Pty,
            InputPermission::Allowed,
            ResizePermission::Denied,
            TerminalSignalPermission::Allowed,
            Some(TerminalSize::new(120, 40).unwrap()),
            ResourceQuantity::new(50),
            ResourceQuantity::new(101),
        )
        .unwrap(),
    ];
    for terminal in excessive {
        assert!(
            compile_sandbox(
                support::binding(1),
                IsolationRequirement::Restricted,
                SandboxOperationClass::Execution,
                contract.clone(),
                requirements_with_terminal(&requirements, terminal),
            )
            .is_err()
        );
    }
}

#[test]
fn terminal_bound_fields_change_canonical_plan_identity() {
    let baseline = support::checked_plan();
    let (contract, requirements) = support::contract_and_requirements(false);
    let changed_terminal = TerminalContract::new(
        contract.terminal().modes(),
        contract.terminal().input(),
        contract.terminal().resize(),
        contract.terminal().signals(),
        TerminalLimits::new(
            Some(TerminalSize::new(201, 100).unwrap()),
            contract.terminal().limits().event_count(),
            contract.terminal().limits().output_bytes(),
        )
        .unwrap(),
    )
    .unwrap();
    let changed_contract = SandboxContract::new(
        contract.filesystem().clone(),
        contract.process().clone(),
        contract.environment().clone(),
        contract.network().clone(),
        contract.secrets().clone(),
        *contract.resources(),
        changed_terminal,
    );
    let changed = compile_sandbox(
        support::binding(1),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        changed_contract,
        requirements,
    )
    .unwrap();
    assert_ne!(baseline.canonical_bytes(), changed.canonical_bytes());
    assert_ne!(baseline.digest(), changed.digest());
}

fn requirements_with_terminal(
    requirements: &SandboxRequirements,
    terminal: TerminalRequirements,
) -> SandboxRequirements {
    SandboxRequirements::new(
        requirements.files().to_vec(),
        requirements.process().clone(),
        requirements.environment().clone(),
        requirements.network().to_vec(),
        requirements.secrets().to_vec(),
        *requirements.resources(),
        terminal,
    )
    .unwrap()
}

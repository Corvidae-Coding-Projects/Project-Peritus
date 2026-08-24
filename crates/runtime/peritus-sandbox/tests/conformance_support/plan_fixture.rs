//! Translation from A2 fixtures to real checked sandbox plans.

use peritus_conformance::SandboxConformanceFixture;
use peritus_sandbox::{
    DescendantPolicy, EnvironmentContract, EnvironmentMode, EnvironmentName,
    EnvironmentRequirements, FileOperation, FileOperationSet, FileRequirement, FilesystemContract,
    FilesystemRule, InputPermission, IsolationRequirement, NetworkContract, NetworkHost,
    NetworkRule, NetworkTarget, PathScope, ProcessContract, ProcessRequirements, ResizePermission,
    RuleEffect, SandboxBinding, SandboxContract, SandboxError, SandboxOperationClass, SandboxPath,
    SandboxRequirements, SecretContract, SecretDelivery, SecretGrant, SecretReference,
    SignalPolicy, TerminalContract, TerminalLimits, TerminalMode, TerminalModes,
    TerminalRequirements, TerminalSignalPermission, TerminalSize, Transport, TreeContainment,
    compile_sandbox,
};
use peritus_types::{
    AcceptanceSpecId, EnvironmentId, Generation, HarnessId, PolicyId, ProcessId, ProviderProfileId,
    ResourceId, ResourceQuantity, RevisionNumber, RevisionTuple, WorkspaceId,
};

#[derive(Clone, Copy)]
pub(super) struct PlanShape {
    pub filesystem: FileShape,
    pub filesystem_write: bool,
    pub environment_secret: bool,
    pub network: NetworkShape,
    pub descendant_limit: u32,
    pub descendant_required: u32,
    pub terminal: TerminalShape,
    pub resource_limit: u64,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum FileShape {
    None,
    Allow,
    Deny,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum NetworkShape {
    None,
    Allow,
    Deny,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum TerminalShape {
    Pipes,
    Pty,
    PtyResize,
}

impl PlanShape {
    pub(super) const fn baseline(limit: u64) -> Self {
        Self {
            filesystem: FileShape::None,
            filesystem_write: false,
            environment_secret: false,
            network: NetworkShape::None,
            descendant_limit: 0,
            descendant_required: 0,
            terminal: TerminalShape::Pipes,
            resource_limit: limit,
        }
    }
}

pub(super) fn checked_plan(
    fixture: &SandboxConformanceFixture,
    shape: PlanShape,
    marker: u8,
) -> Result<peritus_sandbox::CheckedSandboxPlan, SandboxError> {
    checked_plan_from(fixture, shape, marker)
}

pub(super) fn checked_preparation_plan(
    shape: PlanShape,
    marker: u8,
) -> Result<peritus_sandbox::CheckedSandboxPlan, SandboxError> {
    checked_plan_from(&PreparationValues, shape, marker)
}

trait FixtureSource {
    fn filesystem_path(&self) -> &'static str;
    fn environment_name(&self) -> &'static str;
    fn network_host(&self) -> &'static str;
    fn network_port(&self) -> u16;
    fn secret_reference(&self) -> &'static str;
}

impl FixtureSource for SandboxConformanceFixture {
    fn filesystem_path(&self) -> &'static str {
        self.filesystem_path()
    }
    fn environment_name(&self) -> &'static str {
        self.environment_name()
    }
    fn network_host(&self) -> &'static str {
        self.network_host()
    }
    fn network_port(&self) -> u16 {
        self.network_port()
    }
    fn secret_reference(&self) -> &'static str {
        self.secret_reference()
    }
}

struct PreparationValues;

impl FixtureSource for PreparationValues {
    fn filesystem_path(&self) -> &'static str {
        "/workspace/conformance"
    }
    fn environment_name(&self) -> &'static str {
        "PERITUS_SECRET"
    }
    fn network_host(&self) -> &'static str {
        "api.example.invalid"
    }
    fn network_port(&self) -> u16 {
        443
    }
    fn secret_reference(&self) -> &'static str {
        "secret://conformance/preparation"
    }
}

fn checked_plan_from(
    fixture: &impl FixtureSource,
    shape: PlanShape,
    marker: u8,
) -> Result<peritus_sandbox::CheckedSandboxPlan, SandboxError> {
    let filesystem = filesystem(fixture, shape.filesystem, shape.filesystem_write)?;
    let process = ProcessContract::new(
        vec![SandboxPath::new("/bin/peritus-conformance")?],
        if shape.descendant_limit == 0 {
            DescendantPolicy::Denied
        } else {
            DescendantPolicy::Bounded(shape.descendant_limit)
        },
        SignalPolicy::GracefulAndForced,
        TreeContainment::Required,
        shape.descendant_limit.saturating_add(1),
    )?;
    let environment_name = EnvironmentName::new(fixture.environment_name())?;
    let environment = EnvironmentContract::new(
        EnvironmentMode::Cleared,
        if shape.environment_secret { vec![environment_name.clone()] } else { Vec::new() },
    )?;
    let (secret_contract, secret_requirements) = secrets(fixture, shape.environment_secret)?;
    let (network_contract, network_requirements) = network(fixture, shape.network)?;
    let (terminal_contract, terminal_requirements) = terminal(shape.terminal)?;
    let contract = SandboxContract::new(
        filesystem,
        process,
        environment,
        network_contract,
        secret_contract,
        limits(shape.resource_limit)?,
        terminal_contract,
    );
    let file_requirements = if shape.filesystem == FileShape::None {
        Vec::new()
    } else {
        vec![FileRequirement::new(
            SandboxPath::new(fixture.filesystem_path())?,
            if shape.filesystem_write { FileOperation::Write } else { FileOperation::Read },
        )]
    };
    let requirements = SandboxRequirements::new(
        file_requirements,
        ProcessRequirements::new(
            SandboxPath::new("/bin/peritus-conformance")?,
            shape.descendant_required,
            false,
        ),
        EnvironmentRequirements::new(
            Vec::new(),
            if shape.environment_secret { vec![environment_name] } else { Vec::new() },
        )?,
        network_requirements,
        secret_requirements,
        limits(shape.resource_limit)?,
        terminal_requirements,
    )?;
    compile_sandbox(
        binding(marker),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        contract,
        requirements,
    )
}

fn filesystem(
    fixture: &impl FixtureSource,
    shape: FileShape,
    write: bool,
) -> Result<FilesystemContract, SandboxError> {
    if shape == FileShape::None {
        return Ok(FilesystemContract::deny_all());
    }
    let operation = if write {
        FileOperationSet::from_operations([FileOperation::Read, FileOperation::Write])
    } else {
        FileOperationSet::from_operations([FileOperation::Read])
    };
    let mut rules = vec![FilesystemRule::new(
        RuleEffect::Allow,
        SandboxPath::new("/workspace")?,
        PathScope::Descendants,
        operation,
    )?];
    if shape == FileShape::Deny {
        rules.push(FilesystemRule::new(
            RuleEffect::Deny,
            SandboxPath::new(fixture.filesystem_path())?,
            PathScope::Exact,
            operation,
        )?);
    }
    FilesystemContract::new(rules)
}

fn network(
    fixture: &impl FixtureSource,
    shape: NetworkShape,
) -> Result<(NetworkContract, Vec<NetworkTarget>), SandboxError> {
    if shape == NetworkShape::None {
        return Ok((NetworkContract::deny_all(), Vec::new()));
    }
    let host = peritus_sandbox::DnsName::new(fixture.network_host())?;
    let target =
        NetworkTarget::new(NetworkHost::Dns(host.clone()), Transport::Tcp, fixture.network_port())?;
    let rules = if shape == NetworkShape::Allow {
        vec![NetworkRule::new(
            RuleEffect::Allow,
            peritus_sandbox::HostMatcher::DnsExact(host),
            Transport::Tcp,
            peritus_sandbox::PortRange::new(fixture.network_port(), fixture.network_port())?,
        )]
    } else {
        Vec::new()
    };
    Ok((NetworkContract::new(rules)?, vec![target]))
}

fn secrets(
    fixture: &impl FixtureSource,
    enabled: bool,
) -> Result<(SecretContract, Vec<SecretGrant>), SandboxError> {
    if !enabled {
        return Ok((SecretContract::deny_all(), Vec::new()));
    }
    let digest = peritus_codec::sha256(fixture.secret_reference().as_bytes());
    let mut identifier = [0_u8; 16];
    identifier.copy_from_slice(&digest.as_bytes()[..16]);
    if identifier == [0; 16] {
        identifier[0] = 1;
    }
    let grant = SecretGrant::new(
        SecretReference::new(ResourceId::new(identifier).expect("nonzero secret identity"), digest),
        SecretDelivery::Environment(EnvironmentName::new(fixture.environment_name())?),
    );
    Ok((SecretContract::new(vec![grant.clone()])?, vec![grant]))
}

fn terminal(
    shape: TerminalShape,
) -> Result<(TerminalContract, TerminalRequirements), SandboxError> {
    let (mode, resize, size) = match shape {
        TerminalShape::Pipes => (TerminalMode::Pipes, ResizePermission::Denied, None),
        TerminalShape::Pty => {
            (TerminalMode::Pty, ResizePermission::Denied, Some(TerminalSize::new(80, 24)?))
        }
        TerminalShape::PtyResize => {
            (TerminalMode::Pty, ResizePermission::Allowed, Some(TerminalSize::new(80, 24)?))
        }
    };
    Ok((
        TerminalContract::new(
            TerminalModes::from_modes([mode]),
            InputPermission::Denied,
            resize,
            TerminalSignalPermission::Denied,
            TerminalLimits::new(
                if mode == TerminalMode::Pty { Some(TerminalSize::new(200, 100)?) } else { None },
                ResourceQuantity::new(64),
                ResourceQuantity::new(64),
            )?,
        )?,
        TerminalRequirements::new(
            mode,
            InputPermission::Denied,
            resize,
            TerminalSignalPermission::Denied,
            size,
            ResourceQuantity::new(32),
            ResourceQuantity::new(32),
        )?,
    ))
}

pub(super) fn limits(value: u64) -> Result<peritus_sandbox::ResourceLimits, SandboxError> {
    let value = ResourceQuantity::new(value.max(1));
    peritus_sandbox::ResourceLimits::new(value, value, value, value, value, value, value, value)
}

fn binding(seed: u8) -> SandboxBinding {
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new([seed; 16]).expect("nonzero fixture identity"),
        HarnessId::new([seed.wrapping_add(1); 16]).expect("nonzero fixture identity"),
        WorkspaceId::new([seed.wrapping_add(2); 16]).expect("nonzero fixture identity"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new([seed.wrapping_add(3); 16]).expect("nonzero fixture identity"),
        ProviderProfileId::new([seed.wrapping_add(4); 16]).expect("nonzero fixture identity"),
    );
    SandboxBinding::new(
        ProcessId::new([seed.wrapping_add(5); 16]).expect("nonzero fixture identity"),
        ResourceId::new([seed.wrapping_add(6); 16]).expect("nonzero fixture identity"),
        EnvironmentId::new([seed.wrapping_add(7); 16]).expect("nonzero fixture identity"),
        revision,
    )
}

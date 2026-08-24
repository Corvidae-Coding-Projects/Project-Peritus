use peritus_conformance::SandboxConformanceFixture;
use peritus_sandbox::{
    CheckedSandboxPlan, DescendantPolicy, EnvironmentContract, EnvironmentMode, EnvironmentName,
    EnvironmentRequirements, FileOperation, FileOperationSet, FileRequirement, FilesystemContract,
    FilesystemRule, HostMatcher, InputPermission, IsolationRequirement, NetworkContract,
    NetworkHost, NetworkRule, NetworkTarget, PathScope, ProcessContract, ProcessRequirements,
    ResizePermission, ResourceLimits, RuleEffect, SandboxBinding, SandboxContract, SandboxError,
    SandboxOperationClass, SandboxPath, SandboxRequirements, SecretContract, SecretDelivery,
    SecretGrant, SecretReference, SignalPolicy, TerminalContract, TerminalLimits, TerminalMode,
    TerminalModes, TerminalRequirements, TerminalSignalPermission, TerminalSize, Transport,
    TreeContainment, compile_sandbox,
};
use peritus_types::{
    AcceptanceSpecId, EnvironmentId, Generation, HarnessId, PolicyId, ProcessId, ProviderProfileId,
    ResourceId, ResourceQuantity, RevisionNumber, RevisionTuple, WorkspaceId,
};

#[derive(Clone, Copy)]
pub struct PlanShape {
    pub file: FileShape,
    pub file_write: bool,
    pub environment_secret: bool,
    pub network: NetworkShape,
    pub descendants: u32,
    pub required_descendants: u32,
    pub terminal: TerminalShape,
    pub resource_limit: u64,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum FileShape {
    None,
    Allow,
    Deny,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub enum NetworkShape {
    None,
    Allow,
    Deny,
}

#[derive(Clone, Copy)]
pub enum TerminalShape {
    Pipes,
    Pty,
    PtyResize,
}

impl PlanShape {
    pub const fn baseline(resource_limit: u64) -> Self {
        Self {
            file: FileShape::None,
            file_write: false,
            environment_secret: false,
            network: NetworkShape::None,
            descendants: 0,
            required_descendants: 0,
            terminal: TerminalShape::Pipes,
            resource_limit,
        }
    }
}

pub fn checked(
    fixture: &SandboxConformanceFixture,
    shape: PlanShape,
    marker: u8,
) -> Result<CheckedSandboxPlan, SandboxError> {
    build(
        fixture.filesystem_path(),
        fixture.environment_name(),
        fixture.network_host(),
        fixture.network_port(),
        fixture.secret_reference(),
        shape,
        marker,
    )
}

pub fn preparation(shape: PlanShape, marker: u8) -> Result<CheckedSandboxPlan, SandboxError> {
    build(
        "/workspace/conformance",
        "PERITUS_SECRET",
        "api.example.invalid",
        443,
        "secret://conformance/preparation",
        shape,
        marker,
    )
}

#[allow(clippy::too_many_arguments, reason = "closed cross-domain conformance fixture")]
fn build(
    file_path: &str,
    environment_name: &str,
    network_host: &str,
    network_port: u16,
    secret_reference: &str,
    shape: PlanShape,
    marker: u8,
) -> Result<CheckedSandboxPlan, SandboxError> {
    let filesystem = filesystem(file_path, shape.file, shape.file_write)?;
    let program = SandboxPath::new("/bin/peritus-conformance")?;
    let process = ProcessContract::new(
        vec![program.clone()],
        if shape.descendants == 0 {
            DescendantPolicy::Denied
        } else {
            DescendantPolicy::Bounded(shape.descendants)
        },
        SignalPolicy::GracefulAndForced,
        TreeContainment::Required,
        shape.descendants.saturating_add(1),
    )?;
    let secret_name = EnvironmentName::new(environment_name)?;
    let ordinary_name = EnvironmentName::new("PERITUS_MODE")?;
    let environment = EnvironmentContract::new(
        EnvironmentMode::Cleared,
        if shape.environment_secret {
            vec![ordinary_name.clone(), secret_name.clone()]
        } else {
            Vec::new()
        },
    )?;
    let (network_contract, network_requirements) =
        network(network_host, network_port, shape.network)?;
    let (secret_contract, secret_requirements) =
        secrets(secret_reference, secret_name, shape.environment_secret)?;
    let (terminal_contract, terminal_requirements) = terminal(shape.terminal)?;
    let limits = limits(shape.resource_limit)?;
    let contract = SandboxContract::new(
        filesystem,
        process,
        environment,
        network_contract,
        secret_contract,
        limits,
        terminal_contract,
    );
    let files = if shape.file == FileShape::None {
        Vec::new()
    } else {
        vec![FileRequirement::new(
            SandboxPath::new(file_path)?,
            if shape.file_write { FileOperation::Write } else { FileOperation::Read },
        )]
    };
    let environment = EnvironmentRequirements::new(
        Vec::new(),
        if shape.environment_secret { vec![ordinary_name] } else { Vec::new() },
    )?;
    let requirements = SandboxRequirements::new(
        files,
        ProcessRequirements::new(program, shape.required_descendants, false),
        environment,
        network_requirements,
        secret_requirements,
        limits,
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
    path: &str,
    shape: FileShape,
    write: bool,
) -> Result<FilesystemContract, SandboxError> {
    if shape == FileShape::None {
        return Ok(FilesystemContract::deny_all());
    }
    let access = if write {
        FileOperationSet::from_operations([FileOperation::Read, FileOperation::Write])
    } else {
        FileOperationSet::from_operations([FileOperation::Read])
    };
    let mut rules = vec![FilesystemRule::new(
        RuleEffect::Allow,
        SandboxPath::new("/workspace")?,
        PathScope::Descendants,
        access,
    )?];
    if shape == FileShape::Deny {
        rules.push(FilesystemRule::new(
            RuleEffect::Deny,
            SandboxPath::new(path)?,
            PathScope::Exact,
            access,
        )?);
    }
    FilesystemContract::new(rules)
}

fn network(
    name: &str,
    port: u16,
    shape: NetworkShape,
) -> Result<(NetworkContract, Vec<NetworkTarget>), SandboxError> {
    if shape == NetworkShape::None {
        return Ok((NetworkContract::deny_all(), Vec::new()));
    }
    let host = peritus_sandbox::DnsName::new(name)?;
    let target = NetworkTarget::new(NetworkHost::Dns(host.clone()), Transport::Tcp, port)?;
    let rules = if shape == NetworkShape::Allow {
        vec![NetworkRule::new(
            RuleEffect::Allow,
            HostMatcher::DnsExact(host),
            Transport::Tcp,
            peritus_sandbox::PortRange::new(port, port)?,
        )]
    } else {
        Vec::new()
    };
    Ok((NetworkContract::new(rules)?, vec![target]))
}

fn secrets(
    text: &str,
    destination: EnvironmentName,
    enabled: bool,
) -> Result<(SecretContract, Vec<SecretGrant>), SandboxError> {
    if !enabled {
        return Ok((SecretContract::deny_all(), Vec::new()));
    }
    let digest = peritus_codec::sha256(text.as_bytes());
    let mut identifier = [0_u8; 16];
    identifier.copy_from_slice(&digest.as_bytes()[..16]);
    identifier[0] |= 1;
    let grant = SecretGrant::new(
        SecretReference::new(ResourceId::new(identifier).expect("nonzero digest ID"), digest),
        SecretDelivery::Environment(destination),
    );
    Ok((SecretContract::new(vec![grant.clone()])?, vec![grant]))
}

fn terminal(
    shape: TerminalShape,
) -> Result<(TerminalContract, TerminalRequirements), SandboxError> {
    let (mode, resize, initial) = match shape {
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
            initial,
            ResourceQuantity::new(32),
            ResourceQuantity::new(32),
        )?,
    ))
}

pub fn limits(value: u64) -> Result<ResourceLimits, SandboxError> {
    let value = ResourceQuantity::new(value.max(1));
    ResourceLimits::new(value, value, value, value, value, value, value, value)
}

fn binding(seed: u8) -> SandboxBinding {
    let revision = RevisionTuple::new(
        AcceptanceSpecId::new([seed; 16]).expect("acceptance ID"),
        HarnessId::new([seed.wrapping_add(1); 16]).expect("harness ID"),
        WorkspaceId::new([seed.wrapping_add(2); 16]).expect("workspace ID"),
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new([seed.wrapping_add(3); 16]).expect("policy ID"),
        ProviderProfileId::new([seed.wrapping_add(4); 16]).expect("provider ID"),
    );
    SandboxBinding::new(
        ProcessId::new([seed.wrapping_add(5); 16]).expect("process ID"),
        ResourceId::new([seed.wrapping_add(6); 16]).expect("resource ID"),
        EnvironmentId::new([seed.wrapping_add(7); 16]).expect("environment ID"),
        revision,
    )
}

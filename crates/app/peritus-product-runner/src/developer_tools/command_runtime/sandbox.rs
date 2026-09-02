//! Explicit raw-effect sandbox contract used by the C2 product command plan.

use peritus_process::{
    EnvironmentPlan, EnvironmentSource, EnvironmentValueSource, IoMode, ProcessResourcePolicy,
    StdinPolicy,
};
use peritus_sandbox::{
    AdmissionProfile, BackendAdmission, BackendDescriptor, BackendKind, BackendName,
    BackendVersion, CheckedSandboxPlan, DescendantPolicy, EnvironmentContract, EnvironmentMode,
    EnvironmentName, EnvironmentRequirements, FeatureSet, FileOperation, FileOperationSet,
    FileRequirement, FilesystemContract, FilesystemRule, InputPermission, IsolationRequirement,
    NetworkContract, PathScope, PathSemantics, ProcessContract, ProcessRequirements,
    ResizePermission, ResourceFidelity, ResourceLimits, RuleEffect, SandboxBinding,
    SandboxContract, SandboxOperationClass, SandboxPath, SandboxRequirements, SecretContract,
    SignalPolicy, TerminalContract, TerminalLimits, TerminalMode, TerminalModes,
    TerminalRequirements, TerminalSignalPermission, TreeContainment, admit_backend,
    compile_sandbox,
};
use peritus_types::ResourceQuantity;

use super::identity::CommandIds;

pub(super) fn raw_effect(
    ids: &CommandIds,
    executable: &str,
    workspace: &std::path::Path,
    environment: &EnvironmentPlan,
    io: IoMode,
    stdin: StdinPolicy,
    resources: ProcessResourcePolicy,
) -> Result<(CheckedSandboxPlan, BackendAdmission), String> {
    let executable = path(executable)?;
    let workspace = path(&workspace.to_string_lossy())?;
    let (inherited, literals) = environment_names(environment)?;
    let environment_mode = match environment.source() {
        EnvironmentSource::Cleared => EnvironmentMode::Cleared,
        EnvironmentSource::Allowlisted(_) => EnvironmentMode::AllowListed(inherited.clone()),
    };
    let environment_contract = EnvironmentContract::new(environment_mode, literals.clone())
        .map_err(|error| format!("construct command environment contract: {error}"))?;
    let limits = resource_limits(resources)?;
    let (terminal, terminal_requirements) = terminal(io, stdin, resources)?;
    let filesystem = FilesystemContract::new(vec![
        FilesystemRule::new(
            RuleEffect::Allow,
            executable.clone(),
            PathScope::Exact,
            FileOperationSet::from_operations([FileOperation::Execute]),
        )
        .map_err(|error| format!("construct executable filesystem rule: {error}"))?,
        FilesystemRule::new(
            RuleEffect::Allow,
            workspace,
            PathScope::Descendants,
            FileOperationSet::from_operations([
                FileOperation::Discover,
                FileOperation::Metadata,
                FileOperation::Read,
                FileOperation::Execute,
                FileOperation::Create,
                FileOperation::Write,
                FileOperation::Remove,
            ]),
        )
        .map_err(|error| format!("construct workspace filesystem rule: {error}"))?,
    ])
    .map_err(|error| format!("construct command filesystem contract: {error}"))?;
    let descendants =
        u32::try_from(resources.process_count().saturating_sub(1)).unwrap_or(u32::MAX - 1);
    let process = ProcessContract::new(
        vec![executable.clone()],
        DescendantPolicy::Bounded(descendants),
        SignalPolicy::GracefulAndForced,
        TreeContainment::NotRequiredForRawEffect,
        descendants.saturating_add(1),
    )
    .map_err(|error| format!("construct command process contract: {error}"))?;
    let contract = SandboxContract::new(
        filesystem,
        process,
        environment_contract,
        NetworkContract::deny_all(),
        SecretContract::deny_all(),
        limits,
        terminal,
    );
    let requirements = SandboxRequirements::new(
        vec![FileRequirement::new(executable.clone(), FileOperation::Execute)],
        ProcessRequirements::new(executable, descendants, true),
        EnvironmentRequirements::new(inherited, literals)
            .map_err(|error| format!("construct command environment requirements: {error}"))?,
        Vec::new(),
        Vec::new(),
        limits,
        terminal_requirements,
    )
    .map_err(|error| format!("construct command sandbox requirements: {error}"))?;
    let checked = compile_sandbox(
        SandboxBinding::new(ids.process, ids.resource, ids.environment, ids.revision),
        IsolationRequirement::ExplicitRawEffect,
        SandboxOperationClass::RawEffect,
        contract,
        requirements,
    )
    .map_err(|error| format!("compile explicit raw-effect command sandbox: {error}"))?;
    let descriptor = BackendDescriptor::new(
        BackendName::new("peritus-product-raw")
            .map_err(|error| format!("construct command backend name: {error}"))?,
        BackendVersion::new("1")
            .map_err(|error| format!("construct command backend version: {error}"))?,
        BackendKind::ReferenceOnly,
        native_path_semantics(),
        ResourceFidelity::Supervisor,
        FeatureSet::all(),
    );
    let admission = admit_backend(&checked, &descriptor, AdmissionProfile::Conformance)
        .map_err(|error| format!("admit explicit raw-effect command backend: {error}"))?;
    Ok((checked, admission))
}

fn environment_names(
    environment: &EnvironmentPlan,
) -> Result<(Vec<EnvironmentName>, Vec<EnvironmentName>), String> {
    let inherited = match environment.source() {
        EnvironmentSource::Cleared => Vec::new(),
        EnvironmentSource::Allowlisted(names) => names
            .iter()
            .map(|name| {
                EnvironmentName::new(name)
                    .map_err(|error| format!("construct inherited environment name: {error}"))
            })
            .collect::<Result<Vec<_>, _>>()?,
    };
    let mut literals = Vec::new();
    for variable in environment.variables() {
        if variable.source() == EnvironmentValueSource::Literal {
            literals.push(
                EnvironmentName::new(variable.name())
                    .map_err(|error| format!("construct literal environment name: {error}"))?,
            );
        }
    }
    Ok((inherited, literals))
}

fn terminal(
    io: IoMode,
    stdin: StdinPolicy,
    resources: ProcessResourcePolicy,
) -> Result<(TerminalContract, TerminalRequirements), String> {
    let input = if matches!(stdin, StdinPolicy::Closed) {
        InputPermission::Denied
    } else {
        InputPermission::Allowed
    };
    let (mode, size, resize) = match io {
        IoMode::Pipes => (TerminalMode::Pipes, None, ResizePermission::Denied),
        IoMode::Pty(size) => (
            TerminalMode::Pty,
            Some(
                peritus_sandbox::TerminalSize::new(size.columns(), size.rows())
                    .map_err(|error| format!("construct command terminal size: {error}"))?,
            ),
            ResizePermission::Allowed,
        ),
    };
    let event_count = ResourceQuantity::new(16_384);
    let output_bytes = ResourceQuantity::new(resources.output_bytes());
    let limits = TerminalLimits::new(size, event_count, output_bytes)
        .map_err(|error| format!("construct command terminal limits: {error}"))?;
    let contract = TerminalContract::new(
        TerminalModes::from_modes([mode]),
        input,
        resize,
        TerminalSignalPermission::Allowed,
        limits,
    )
    .map_err(|error| format!("construct command terminal contract: {error}"))?;
    let requirements = TerminalRequirements::new(
        mode,
        input,
        resize,
        TerminalSignalPermission::Allowed,
        size,
        event_count,
        output_bytes,
    )
    .map_err(|error| format!("construct command terminal requirements: {error}"))?;
    Ok((contract, requirements))
}

fn resource_limits(resources: ProcessResourcePolicy) -> Result<ResourceLimits, String> {
    ResourceLimits::new(
        ResourceQuantity::new(resources.wall_millis()),
        ResourceQuantity::new(resources.cpu_millis()),
        ResourceQuantity::new(resources.memory_bytes()),
        ResourceQuantity::new(resources.disk_bytes()),
        ResourceQuantity::new(resources.output_bytes()),
        ResourceQuantity::new(resources.file_descriptors()),
        ResourceQuantity::new(resources.process_count()),
        ResourceQuantity::new(resources.concurrent_slots()),
    )
    .map_err(|error| format!("construct command resource contract: {error}"))
}

fn path(value: &str) -> Result<SandboxPath, String> {
    SandboxPath::new(value.replace('\\', "/"))
        .map_err(|error| format!("construct command sandbox path: {error}"))
}

const fn native_path_semantics() -> PathSemantics {
    #[cfg(unix)]
    {
        PathSemantics::UnixNative
    }
    #[cfg(windows)]
    {
        PathSemantics::WindowsNative
    }
}

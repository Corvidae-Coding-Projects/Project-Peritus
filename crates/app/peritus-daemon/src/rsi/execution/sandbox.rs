//! Private filesystem, cleared environment, credential and network denial, and native limits.

use std::path::Path;

use peritus_process::{ExecutionIdentity, ProcessResourcePolicy};
use peritus_sandbox::{
    CheckedSandboxPlan, DescendantPolicy, EnvironmentContract, EnvironmentMode,
    EnvironmentRequirements, FileOperation, FileOperationSet, FileRequirement, FilesystemContract,
    FilesystemRule, InputPermission, IsolationRequirement, NetworkContract, PathScope,
    ProcessContract, ProcessRequirements, ResizePermission, ResourceLimits, RuleEffect,
    SandboxBinding, SandboxContract, SandboxOperationClass, SandboxPath, SandboxRequirements,
    SecretContract, SignalPolicy, TerminalContract, TerminalLimits, TerminalMode, TerminalModes,
    TerminalRequirements, TerminalSignalPermission, TreeContainment, compile_sandbox,
};
use peritus_types::ResourceQuantity;

use super::{detail, materialize::Materialization};

pub(super) fn compile(
    identity: ExecutionIdentity,
    materialized: &Materialization,
    resources: ProcessResourcePolicy,
) -> Result<CheckedSandboxPlan, String> {
    let executable = path(&materialized.executable)?;
    let limits = ResourceLimits::with_optional_wall_output_cpu(
        resources.wall_millis().map(ResourceQuantity::new),
        resources.output_bytes().map(ResourceQuantity::new),
        resources.cpu_millis().map(ResourceQuantity::new),
        ResourceQuantity::new(resources.memory_bytes()),
        ResourceQuantity::new(resources.disk_bytes()),
        ResourceQuantity::new(resources.file_descriptors()),
        ResourceQuantity::new(resources.process_count()),
        ResourceQuantity::new(resources.concurrent_slots()),
    )
    .map_err(detail)?;
    let output =
        ResourceQuantity::new(resources.output_bytes().ok_or("RSI output ceiling absent")?);
    let events = ResourceQuantity::new(8192);
    let descendants = u32::try_from(resources.process_count() - 1)
        .map_err(|_| "RSI process ceiling exceeds native representation")?;
    let terminal = TerminalContract::new(
        TerminalModes::from_modes([TerminalMode::Pipes]),
        InputPermission::Denied,
        ResizePermission::Denied,
        TerminalSignalPermission::Allowed,
        TerminalLimits::new(None, events, output).map_err(detail)?,
    )
    .map_err(detail)?;
    let contract = SandboxContract::new(
        filesystem(materialized)?,
        ProcessContract::new(
            vec![executable.clone()],
            DescendantPolicy::Bounded(descendants),
            SignalPolicy::GracefulAndForced,
            TreeContainment::Required,
            descendants + 1,
        )
        .map_err(detail)?,
        EnvironmentContract::new(EnvironmentMode::Cleared, Vec::new()).map_err(detail)?,
        NetworkContract::deny_all(),
        SecretContract::deny_all(),
        limits,
        terminal,
    );
    let requirements = SandboxRequirements::new(
        vec![FileRequirement::new(executable.clone(), FileOperation::Execute)],
        ProcessRequirements::new(executable, descendants, true),
        EnvironmentRequirements::new(Vec::new(), Vec::new()).map_err(detail)?,
        Vec::new(),
        Vec::new(),
        limits,
        TerminalRequirements::new(
            TerminalMode::Pipes,
            InputPermission::Denied,
            ResizePermission::Denied,
            TerminalSignalPermission::Allowed,
            None,
            events,
            output,
        )
        .map_err(detail)?,
    )
    .map_err(detail)?;
    compile_sandbox(
        SandboxBinding::new(
            identity.process_id(),
            identity.resource_id(),
            identity.environment_id(),
            identity.revision(),
        ),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        contract,
        requirements,
    )
    .map_err(detail)
}

fn filesystem(materialized: &Materialization) -> Result<FilesystemContract, String> {
    let mut rules = Vec::new();
    for file in &materialized.files {
        let mut operations = vec![FileOperation::Read, FileOperation::Metadata];
        if file.executable {
            operations.push(FileOperation::Execute);
        }
        rules.push(
            FilesystemRule::new(
                RuleEffect::Allow,
                path(&file.path)?,
                PathScope::Exact,
                FileOperationSet::from_operations(operations),
            )
            .map_err(detail)?,
        );
    }
    rules.push(
        FilesystemRule::new(
            RuleEffect::Allow,
            path(&materialized.work)?,
            PathScope::Descendants,
            FileOperationSet::from_operations([
                FileOperation::Read,
                FileOperation::Discover,
                FileOperation::Metadata,
                FileOperation::Create,
                FileOperation::Write,
                FileOperation::Remove,
            ]),
        )
        .map_err(detail)?,
    );
    // Only fixed OS library trees, never user home, daemon state, /usr/bin, or another arm.
    for runtime in ["/usr/lib", "/usr/lib64", "/lib", "/lib64"] {
        let Ok(runtime) = Path::new(runtime).canonicalize() else {
            continue;
        };
        rules.push(
            FilesystemRule::new(
                RuleEffect::Allow,
                path(&runtime)?,
                PathScope::Descendants,
                FileOperationSet::from_operations([
                    FileOperation::Read,
                    FileOperation::Discover,
                    FileOperation::Metadata,
                    FileOperation::Execute,
                ]),
            )
            .map_err(detail)?,
        );
    }
    FilesystemContract::new(rules).map_err(detail)
}

fn path(path: &Path) -> Result<SandboxPath, String> {
    SandboxPath::new(path.to_str().ok_or("RSI sandbox path is not UTF-8")?).map_err(detail)
}

//! A cleared-environment, no-network, no-secret contract admitting only inference inputs.

use super::{detail, identity::CommandIds};
use peritus_process::ProcessResourcePolicy;
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
use std::path::{Path, PathBuf};

fn filesystem(
    directory: &Path,
    executable: &Path,
    weights: &Path,
) -> Result<FilesystemContract, String> {
    let executable = path(executable)?;
    let read = FileOperationSet::from_operations([
        FileOperation::Read,
        FileOperation::Discover,
        FileOperation::Metadata,
    ]);
    let mut rules = vec![
        FilesystemRule::new(
            RuleEffect::Allow,
            executable,
            PathScope::Exact,
            FileOperationSet::from_operations([
                FileOperation::Read,
                FileOperation::Execute,
                FileOperation::Metadata,
            ]),
        )
        .map_err(detail)?,
        FilesystemRule::new(RuleEffect::Allow, path(weights)?, PathScope::Exact, read)
            .map_err(detail)?,
        FilesystemRule::new(
            RuleEffect::Allow,
            path(directory)?,
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
    ];
    // Runtime library trees are read-only; neither the task workspace nor user home is mounted.
    for root in runtime_roots() {
        if root.exists() {
            let root = root.canonicalize().map_err(|_| "resolve installed runtime library root")?;
            rules.push(
                FilesystemRule::new(
                    RuleEffect::Allow,
                    path(&root)?,
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
    }
    FilesystemContract::new(rules).map_err(detail)
}

pub(super) fn compile(
    ids: &CommandIds,
    directory: &Path,
    executable: &Path,
    weights: &Path,
    resources: ProcessResourcePolicy,
) -> Result<CheckedSandboxPlan, String> {
    let filesystem = filesystem(directory, executable, weights)?;
    let executable = path(executable)?;
    let resources = ResourceLimits::new(
        ResourceQuantity::new(resources.wall_millis()),
        ResourceQuantity::new(resources.cpu_millis()),
        ResourceQuantity::new(resources.memory_bytes()),
        ResourceQuantity::new(resources.disk_bytes()),
        ResourceQuantity::new(resources.output_bytes()),
        ResourceQuantity::new(resources.file_descriptors()),
        ResourceQuantity::new(resources.process_count()),
        ResourceQuantity::new(resources.concurrent_slots()),
    )
    .map_err(detail)?;
    let events = ResourceQuantity::new(8192);
    let output = resources.limit(peritus_sandbox::SandboxResourceKind::Output);
    let terminal = TerminalContract::new(
        TerminalModes::from_modes([TerminalMode::Pipes]),
        InputPermission::Allowed,
        ResizePermission::Denied,
        TerminalSignalPermission::Allowed,
        TerminalLimits::new(None, events, output).map_err(detail)?,
    )
    .map_err(detail)?;
    let contract = SandboxContract::new(
        filesystem,
        ProcessContract::new(
            vec![executable.clone()],
            DescendantPolicy::Bounded(31),
            SignalPolicy::GracefulAndForced,
            TreeContainment::Required,
            32,
        )
        .map_err(detail)?,
        EnvironmentContract::new(EnvironmentMode::Cleared, vec![]).map_err(detail)?,
        NetworkContract::deny_all(),
        SecretContract::deny_all(),
        resources,
        terminal,
    );
    let requirements = SandboxRequirements::new(
        vec![FileRequirement::new(executable.clone(), FileOperation::Execute)],
        ProcessRequirements::new(executable, 31, true),
        EnvironmentRequirements::new(vec![], vec![]).map_err(detail)?,
        vec![],
        vec![],
        resources,
        TerminalRequirements::new(
            TerminalMode::Pipes,
            InputPermission::Allowed,
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
        SandboxBinding::new(ids.process, ids.resource, ids.environment, ids.revision),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        contract,
        requirements,
    )
    .map_err(detail)
}

fn path(path: &Path) -> Result<SandboxPath, String> {
    SandboxPath::new(normalized_path(path)?).map_err(detail)
}

pub(super) fn normalized_path(path: &Path) -> Result<String, String> {
    let text = path.to_str().ok_or("local sandbox path is not UTF-8")?;
    #[cfg(target_os = "windows")]
    let text = text.strip_prefix(r"\\?\").unwrap_or(text);
    Ok(text.replace('\\', "/"))
}

fn runtime_roots() -> Vec<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        ["/usr/lib", "/usr/lib64", "/lib", "/lib64"].iter().map(PathBuf::from).collect()
    }
    #[cfg(target_os = "macos")]
    {
        ["/usr/lib", "/System/Library"].iter().map(PathBuf::from).collect()
    }
    #[cfg(target_os = "windows")]
    {
        // AppContainer's native runtime baseline supplies OS libraries; no broad mutable ACL
        // entries are installed on System32. User-provided inputs remain explicitly scoped.
        Vec::new()
    }
}

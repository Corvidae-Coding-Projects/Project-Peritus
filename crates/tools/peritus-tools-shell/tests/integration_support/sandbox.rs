//! Exact restricted sandbox fixture projection.

use std::path::Path;

use peritus_process::{EnvironmentPlan, ProcessResourcePolicy, StdinPolicy};
use peritus_sandbox::{
    AdmissionProfile, BackendAdmission, BackendDescriptor, BackendKind, BackendName,
    BackendVersion, CheckedSandboxPlan, DescendantPolicy, EnvironmentContract, EnvironmentMode,
    EnvironmentRequirements, FeatureSet, FileOperation, FileOperationSet, FileRequirement,
    FilesystemContract, FilesystemRule, InputPermission, IsolationRequirement, NetworkContract,
    PathScope, PathSemantics, ProcessContract, ProcessRequirements, ResizePermission,
    ResourceFidelity, ResourceLimits, RuleEffect, SandboxBinding, SandboxContract,
    SandboxOperationClass, SandboxPath, SandboxRequirements, SecretContract, SignalPolicy,
    TerminalContract, TerminalLimits, TerminalMode, TerminalModes, TerminalRequirements,
    TerminalSignalPermission, TreeContainment, admit_backend, compile_sandbox,
};
use peritus_types::ResourceQuantity;

use crate::process_authority::Ids;

pub fn sandbox(
    ids: &Ids,
    workspace: &Path,
    executable: &str,
    environment: &EnvironmentPlan,
    resources: ProcessResourcePolicy,
    stdin: StdinPolicy,
) -> (CheckedSandboxPlan, BackendAdmission) {
    let executable = SandboxPath::new(executable).expect("sandbox executable");
    let workspace = SandboxPath::new(workspace.to_string_lossy()).expect("sandbox workspace");
    let filesystem = execution_filesystem(&executable, workspace);
    let process = execution_process(&executable);
    let literal_names = environment
        .variables()
        .iter()
        .map(|value| peritus_sandbox::EnvironmentName::new(value.name()).expect("environment name"))
        .collect::<Vec<_>>();
    let environment_contract =
        EnvironmentContract::new(EnvironmentMode::Cleared, literal_names.clone())
            .expect("environment contract");
    let limits = resource_limits(resources);
    let input = if matches!(stdin, StdinPolicy::Closed) {
        InputPermission::Denied
    } else {
        InputPermission::Allowed
    };
    let terminal_limits = TerminalLimits::new(
        None,
        ResourceQuantity::new(512),
        ResourceQuantity::new(resources.output_bytes()),
    )
    .expect("terminal limits");
    let terminal = TerminalContract::new(
        TerminalModes::from_modes([TerminalMode::Pipes]),
        input,
        ResizePermission::Denied,
        TerminalSignalPermission::Allowed,
        terminal_limits,
    )
    .expect("terminal contract");
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
        ProcessRequirements::new(executable, 0, true),
        EnvironmentRequirements::new(Vec::new(), literal_names).expect("environment requirements"),
        Vec::new(),
        Vec::new(),
        limits,
        TerminalRequirements::new(
            TerminalMode::Pipes,
            input,
            ResizePermission::Denied,
            TerminalSignalPermission::Allowed,
            None,
            ResourceQuantity::new(512),
            ResourceQuantity::new(resources.output_bytes()),
        )
        .expect("terminal requirements"),
    )
    .expect("sandbox requirements");
    let checked = compile_sandbox(
        SandboxBinding::new(ids.process, ids.resource, ids.environment, ids.revision),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        contract,
        requirements,
    )
    .expect("checked sandbox");
    let descriptor = BackendDescriptor::new(
        BackendName::new("peritus-c4-native-test").expect("backend name"),
        BackendVersion::new("1").expect("backend version"),
        BackendKind::Native,
        native_path_semantics(),
        ResourceFidelity::Hard,
        FeatureSet::all(),
    );
    let admission = admit_backend(&checked, &descriptor, AdmissionProfile::Production)
        .expect("backend admission");
    (checked, admission)
}

fn execution_filesystem(executable: &SandboxPath, workspace: SandboxPath) -> FilesystemContract {
    FilesystemContract::new(vec![
        FilesystemRule::new(
            RuleEffect::Allow,
            executable.clone(),
            PathScope::Exact,
            FileOperationSet::from_operations([FileOperation::Execute]),
        )
        .expect("executable rule"),
        FilesystemRule::new(
            RuleEffect::Allow,
            workspace,
            PathScope::Descendants,
            FileOperationSet::from_operations([FileOperation::Read, FileOperation::Metadata]),
        )
        .expect("workspace read rule"),
    ])
    .expect("filesystem contract")
}

fn execution_process(executable: &SandboxPath) -> ProcessContract {
    ProcessContract::new(
        vec![executable.clone()],
        DescendantPolicy::Denied,
        SignalPolicy::GracefulAndForced,
        TreeContainment::Required,
        1,
    )
    .expect("process contract")
}

fn resource_limits(resources: ProcessResourcePolicy) -> ResourceLimits {
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
    .expect("resource limits")
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

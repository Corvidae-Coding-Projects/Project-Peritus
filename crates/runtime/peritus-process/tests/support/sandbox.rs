//! Checked sandbox fixtures projected into process plans.

use peritus_process::{EnvironmentPlan, IoMode, ProcessResourcePolicy, StdinPolicy};
use peritus_sandbox::{
    AdmissionProfile, BackendDescriptor, BackendKind, BackendName, BackendVersion,
    DescendantPolicy, EnvironmentContract, EnvironmentMode, EnvironmentName,
    EnvironmentRequirements, FeatureSet, FileOperation, FileOperationSet, FileRequirement,
    FilesystemContract, FilesystemRule, InputPermission, IsolationRequirement, NetworkContract,
    PathScope, PathSemantics, ProcessContract, ProcessRequirements, ResizePermission,
    ResourceFidelity, ResourceLimits, RuleEffect, SandboxBinding, SandboxContract,
    SandboxOperationClass, SandboxPath, SandboxRequirements, SecretContract, SignalPolicy,
    TerminalContract, TerminalLimits, TerminalMode, TerminalModes, TerminalRequirements,
    TerminalSignalPermission, TreeContainment, admit_backend, compile_sandbox,
};
use peritus_types::ResourceQuantity;

use super::Ids;

pub(super) struct Projection {
    io: IoMode,
    stdin: StdinPolicy,
    resources: ProcessResourcePolicy,
    resource_fidelity: ResourceFidelity,
    descendants: u32,
    resize_allowed: bool,
    inherited: Vec<EnvironmentName>,
    literals: Vec<EnvironmentName>,
}

impl Projection {
    pub(super) fn literal(
        environment: &EnvironmentPlan,
        io: IoMode,
        stdin: StdinPolicy,
        resources: ProcessResourcePolicy,
        descendants: u32,
        resource_fidelity: ResourceFidelity,
    ) -> Self {
        let literals =
            names(environment.variables().iter().map(peritus_process::EnvironmentVariable::name));
        Self {
            io,
            stdin,
            resources,
            resource_fidelity,
            descendants,
            resize_allowed: true,
            inherited: Vec::new(),
            literals,
        }
    }

    pub(super) const fn with_resize(mut self, allowed: bool) -> Self {
        self.resize_allowed = allowed;
        self
    }

    pub(super) fn with_environment(
        mut self,
        inherited: impl IntoIterator<Item = impl AsRef<str>>,
        literals: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Self {
        self.inherited = names(inherited);
        self.literals = names(literals);
        self
    }
}

pub(super) fn compile(
    ids: &Ids,
    executable: &str,
    projection: Projection,
) -> (peritus_sandbox::CheckedSandboxPlan, peritus_sandbox::BackendAdmission) {
    let executable = SandboxPath::new(executable).expect("sandbox executable");
    let filesystem = filesystem(&executable);
    let process = process(&executable, projection.descendants);
    let mode = if projection.inherited.is_empty() {
        EnvironmentMode::Cleared
    } else {
        EnvironmentMode::AllowListed(projection.inherited.clone())
    };
    let environment =
        EnvironmentContract::new(mode, projection.literals.clone()).expect("environment contract");
    let limits = limits(projection.resources);
    let (terminal, terminal_requirements) = terminal(&projection);
    let contract = SandboxContract::new(
        filesystem,
        process,
        environment,
        NetworkContract::deny_all(),
        SecretContract::deny_all(),
        limits,
        terminal,
    );
    let requirements = SandboxRequirements::new(
        vec![FileRequirement::new(executable.clone(), FileOperation::Execute)],
        ProcessRequirements::new(executable, projection.descendants, true),
        EnvironmentRequirements::new(projection.inherited, projection.literals)
            .expect("environment requirements"),
        Vec::new(),
        Vec::new(),
        limits,
        terminal_requirements,
    )
    .expect("sandbox requirements");
    admitted(ids, contract, requirements, projection.resource_fidelity)
}

fn admitted(
    ids: &Ids,
    contract: SandboxContract,
    requirements: SandboxRequirements,
    resource_fidelity: ResourceFidelity,
) -> (peritus_sandbox::CheckedSandboxPlan, peritus_sandbox::BackendAdmission) {
    let checked = compile_sandbox(
        SandboxBinding::new(ids.process, ids.resource, ids.environment, ids.revision),
        IsolationRequirement::ExplicitRawEffect,
        SandboxOperationClass::RawEffect,
        contract,
        requirements,
    )
    .expect("checked sandbox");
    let descriptor = BackendDescriptor::new(
        BackendName::new("peritus-local-test").expect("backend name"),
        BackendVersion::new("1").expect("backend version"),
        BackendKind::ReferenceOnly,
        PathSemantics::LogicalUtf8,
        resource_fidelity,
        FeatureSet::all(),
    );
    let admission = admit_backend(&checked, &descriptor, AdmissionProfile::Conformance)
        .expect("backend admission");
    (checked, admission)
}

fn filesystem(executable: &SandboxPath) -> FilesystemContract {
    FilesystemContract::new(vec![
        FilesystemRule::new(
            RuleEffect::Allow,
            executable.clone(),
            PathScope::Exact,
            FileOperationSet::from_operations([FileOperation::Execute]),
        )
        .expect("execute rule"),
    ])
    .expect("filesystem contract")
}

fn process(executable: &SandboxPath, descendants: u32) -> ProcessContract {
    ProcessContract::new(
        vec![executable.clone()],
        if descendants == 0 {
            DescendantPolicy::Denied
        } else {
            DescendantPolicy::Bounded(descendants)
        },
        SignalPolicy::GracefulAndForced,
        TreeContainment::Required,
        descendants.saturating_add(1),
    )
    .expect("process contract")
}

fn terminal(projection: &Projection) -> (TerminalContract, TerminalRequirements) {
    let input = if matches!(projection.stdin, StdinPolicy::Closed) {
        InputPermission::Denied
    } else {
        InputPermission::Allowed
    };
    let (mode, initial_size, resize) = match projection.io {
        IoMode::Pipes => (TerminalMode::Pipes, None, ResizePermission::Denied),
        IoMode::Pty(size) => (
            TerminalMode::Pty,
            Some(peritus_sandbox::TerminalSize::new(size.columns(), size.rows()).expect("size")),
            if projection.resize_allowed {
                ResizePermission::Allowed
            } else {
                ResizePermission::Denied
            },
        ),
    };
    let event_count = ResourceQuantity::new(256);
    let output_bytes = ResourceQuantity::new(projection.resources.output_bytes());
    let terminal_limits =
        TerminalLimits::new(initial_size, event_count, output_bytes).expect("terminal limits");
    let contract = TerminalContract::new(
        TerminalModes::from_modes([mode]),
        input,
        resize,
        TerminalSignalPermission::Allowed,
        terminal_limits,
    )
    .expect("terminal contract");
    let requirements = TerminalRequirements::new(
        mode,
        input,
        resize,
        TerminalSignalPermission::Allowed,
        initial_size,
        event_count,
        output_bytes,
    )
    .expect("terminal requirements");
    (contract, requirements)
}

fn limits(resources: ProcessResourcePolicy) -> ResourceLimits {
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
    .expect("sandbox limits")
}

fn names(values: impl IntoIterator<Item = impl AsRef<str>>) -> Vec<EnvironmentName> {
    values
        .into_iter()
        .map(|name| EnvironmentName::new(name.as_ref()).expect("sandbox environment"))
        .collect()
}

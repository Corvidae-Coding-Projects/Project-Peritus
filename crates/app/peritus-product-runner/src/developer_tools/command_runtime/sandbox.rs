//! Explicit raw-effect sandbox contract used by the C2 product command plan.

use std::{ffi::OsString, fmt::Write as _, path::Path};

use peritus_process::{
    CommandSpec, EnvironmentPlan, EnvironmentSource, EnvironmentValueSource, IoMode,
    ProcessResourcePolicy, StdinPolicy,
};
use peritus_sandbox::{
    AdmissionProfile, BackendAdmission, BackendDescriptor, BackendKind, BackendName,
    BackendVersion, CheckedSandboxPlan, DescendantPolicy, EnvironmentContract, EnvironmentMode,
    EnvironmentName, EnvironmentRequirements, FeatureSet, FileOperation, FileOperationSet,
    FileRequirement, FilesystemContract, FilesystemRule, InputPermission, IsolationRequirement,
    NativeExecutionAuthority, NetworkContract, PathScope, PathSemantics, ProcessContract,
    ProcessRequirements, ResizePermission, ResourceFidelity, ResourceLimits, RuleEffect,
    SandboxBinding, SandboxContract, SandboxOperationClass, SandboxPath, SandboxRequirements,
    SecretContract, SignalPolicy, TerminalContract, TerminalLimits, TerminalMode, TerminalModes,
    TerminalRequirements, TerminalSignalPermission, TreeContainment, admit_backend,
    compile_sandbox,
};
use peritus_types::ResourceQuantity;

use super::identity::CommandIds;

pub(super) fn raw_effect(
    ids: &CommandIds,
    command: &CommandSpec,
    workspace: &Path,
    environment: &EnvironmentPlan,
    io: IoMode,
    stdin: StdinPolicy,
    resources: ProcessResourcePolicy,
) -> Result<(CheckedSandboxPlan, BackendAdmission), String> {
    let (executable, executable_legacy) = native_path(
        command.executable(),
        "program",
        "construct command executable sandbox path",
    )?;
    let (workspace_path, workspace_legacy) = native_path(
        workspace.as_os_str(),
        "workspace",
        "construct command workspace sandbox path",
    )?;
    let names = environment_names(environment)?;
    let native_authority_required =
        !executable_legacy || !workspace_legacy || !names.legacy;
    let (inherited, literals) = if native_authority_required {
        (
            native_environment_shadows(&names.native_inherited)?,
            native_environment_shadows(&names.native_literals)?,
        )
    } else {
        (names.legacy_inherited, names.legacy_literals)
    };
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
            workspace_path,
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
    let process = ProcessContract::with_optional_process_limit(
        vec![executable.clone()],
        DescendantPolicy::Allowed,
        SignalPolicy::GracefulAndForced,
        TreeContainment::NotRequiredForRawEffect,
        None,
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
        ProcessRequirements::new(executable, 0, true),
        EnvironmentRequirements::new(inherited, literals)
            .map_err(|error| format!("construct command environment requirements: {error}"))?,
        Vec::new(),
        Vec::new(),
        limits,
        terminal_requirements,
    )
    .map_err(|error| format!("construct command sandbox requirements: {error}"))?;
    let mut checked = compile_sandbox(
        SandboxBinding::new(ids.process, ids.resource, ids.environment, ids.revision),
        IsolationRequirement::ExplicitRawEffect,
        SandboxOperationClass::RawEffect,
        contract,
        requirements,
    )
    .map_err(|error| format!("compile explicit raw-effect command sandbox: {error}"))?;
    if native_authority_required {
        checked = checked
            .bind_native_execution(
                NativeExecutionAuthority::new(
                    command.executable().to_os_string(),
                    workspace.to_path_buf(),
                    names.native_inherited,
                    names.native_literals,
                )
                .map_err(|error| format!("construct exact native command authority: {error}"))?,
            )
            .map_err(|error| format!("bind exact native command authority: {error}"))?;
    }
    let descriptor = BackendDescriptor::new(
        BackendName::new("peritus-product-raw")
            .map_err(|error| format!("construct command backend name: {error}"))?,
        BackendVersion::new("1")
            .map_err(|error| format!("construct command backend version: {error}"))?,
        BackendKind::ReferenceOnly,
        native_path_semantics(),
        raw_resource_fidelity(),
        FeatureSet::all(),
    );
    let admission = admit_backend(&checked, &descriptor, AdmissionProfile::Conformance)
        .map_err(|error| format!("admit explicit raw-effect command backend: {error}"))?;
    Ok((checked, admission))
}

pub(super) fn local_compactor(
    ids: &CommandIds,
    command: &CommandSpec,
    workspace: &Path,
    model: &Path,
    environment: &EnvironmentPlan,
    io: IoMode,
    stdin: StdinPolicy,
    resources: ProcessResourcePolicy,
) -> Result<CheckedSandboxPlan, String> {
    let (executable, executable_legacy) = native_path(
        command.executable(),
        "program",
        "construct local compactor executable sandbox path",
    )?;
    let (workspace_path, workspace_legacy) = native_path(
        workspace.as_os_str(),
        "workspace",
        "construct local compactor workspace sandbox path",
    )?;
    let (model_path, _) = native_path(
        model.as_os_str(),
        "model",
        "construct local compactor model sandbox path",
    )?;
    let names = environment_names(environment)?;
    let native_authority_required =
        !executable_legacy || !workspace_legacy || !names.legacy;
    let (inherited, literals) = if native_authority_required {
        (
            native_environment_shadows(&names.native_inherited)?,
            native_environment_shadows(&names.native_literals)?,
        )
    } else {
        (names.legacy_inherited.clone(), names.legacy_literals.clone())
    };
    let environment_mode = match environment.source() {
        EnvironmentSource::Cleared => EnvironmentMode::Cleared,
        EnvironmentSource::Allowlisted(_) => EnvironmentMode::AllowListed(inherited.clone()),
    };
    let environment_contract = EnvironmentContract::new(environment_mode, literals.clone())
        .map_err(|error| format!("construct local compactor environment contract: {error}"))?;
    let limits = resource_limits(resources)?;
    let (terminal, terminal_requirements) = terminal(io, stdin, resources)?;
    let filesystem = FilesystemContract::new(vec![
        FilesystemRule::new(
            RuleEffect::Allow,
            executable.clone(),
            PathScope::Exact,
            FileOperationSet::from_operations([
                FileOperation::Metadata,
                FileOperation::Execute,
            ]),
        )
        .map_err(|error| format!("construct local executable filesystem rule: {error}"))?,
        FilesystemRule::new(
            RuleEffect::Allow,
            model_path.clone(),
            PathScope::Exact,
            FileOperationSet::from_operations([
                FileOperation::Metadata,
                FileOperation::Read,
            ]),
        )
        .map_err(|error| format!("construct local model filesystem rule: {error}"))?,
        FilesystemRule::new(
            RuleEffect::Allow,
            workspace_path,
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
        .map_err(|error| format!("construct local workspace filesystem rule: {error}"))?,
    ])
    .map_err(|error| format!("construct local compactor filesystem contract: {error}"))?;
    let process = ProcessContract::with_optional_process_limit(
        vec![executable.clone()],
        DescendantPolicy::Allowed,
        SignalPolicy::GracefulAndForced,
        TreeContainment::Required,
        None,
    )
    .map_err(|error| format!("construct local compactor process contract: {error}"))?;
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
        vec![
            FileRequirement::new(executable.clone(), FileOperation::Execute),
            FileRequirement::new(model_path, FileOperation::Read),
        ],
        ProcessRequirements::new(executable, 0, true),
        EnvironmentRequirements::new(inherited, literals).map_err(|error| {
            format!("construct local compactor environment requirements: {error}")
        })?,
        Vec::new(),
        Vec::new(),
        limits,
        terminal_requirements,
    )
    .map_err(|error| format!("construct local compactor sandbox requirements: {error}"))?;
    let mut checked = compile_sandbox(
        SandboxBinding::new(ids.process, ids.resource, ids.environment, ids.revision),
        IsolationRequirement::Restricted,
        SandboxOperationClass::Execution,
        contract,
        requirements,
    )
    .map_err(|error| format!("compile restricted local compactor sandbox: {error}"))?;
    if native_authority_required {
        checked = checked
            .bind_native_execution(
                NativeExecutionAuthority::new(
                    command.executable().to_os_string(),
                    workspace.to_path_buf(),
                    names.native_inherited,
                    names.native_literals,
                )
                .map_err(|error| {
                    format!("construct exact native local compactor authority: {error}")
                })?,
            )
            .map_err(|error| format!("bind exact native local compactor authority: {error}"))?;
    }
    Ok(checked)
}

#[cfg(test)]
pub(super) fn compile(
    ids: &CommandIds,
    workspace: &Path,
    executable: &Path,
    model: &Path,
    resources: ProcessResourcePolicy,
) -> Result<CheckedSandboxPlan, String> {
    let command = CommandSpec::new(
        executable.as_os_str().to_os_string(),
        std::iter::empty::<OsString>(),
    )
    .map_err(|error| format!("construct local compactor test command: {error}"))?;
    let environment = EnvironmentPlan::cleared(Vec::new())
        .map_err(|error| format!("construct local compactor test environment: {error}"))?;
    let stdin = StdinPolicy::streaming(1)
        .map_err(|error| format!("construct local compactor test input: {error}"))?;
    local_compactor(
        ids,
        &command,
        workspace,
        model,
        &environment,
        IoMode::Pipes,
        stdin,
        resources,
    )
}

pub(super) fn normalized_path(path: &Path) -> Result<String, String> {
    let text = path.to_str().ok_or("sandbox path is not UTF-8")?;
    #[cfg(windows)]
    let text = text.strip_prefix(r"\\?\").unwrap_or(text);
    Ok(text.replace('\\', "/"))
}

struct EnvironmentNames {
    native_inherited: Vec<OsString>,
    native_literals: Vec<OsString>,
    legacy_inherited: Vec<EnvironmentName>,
    legacy_literals: Vec<EnvironmentName>,
    legacy: bool,
}

fn environment_names(environment: &EnvironmentPlan) -> Result<EnvironmentNames, String> {
    let native_inherited = match environment.source() {
        EnvironmentSource::Cleared => Vec::new(),
        EnvironmentSource::Allowlisted(names) => names.clone(),
    };
    let mut native_literals = Vec::new();
    for variable in environment.variables() {
        if variable.source() == EnvironmentValueSource::Literal {
            native_literals.push(variable.name().to_os_string());
        }
    }
    let legacy_inherited = legacy_environment_names(&native_inherited);
    let legacy_literals = legacy_environment_names(&native_literals);
    let legacy = legacy_inherited.is_some() && legacy_literals.is_some();
    Ok(EnvironmentNames {
        native_inherited,
        native_literals,
        legacy_inherited: legacy_inherited.unwrap_or_default(),
        legacy_literals: legacy_literals.unwrap_or_default(),
        legacy,
    })
}

fn legacy_environment_names(names: &[OsString]) -> Option<Vec<EnvironmentName>> {
    let mut projected = names
        .iter()
        .map(|name| {
            let value = EnvironmentName::new(name.to_str()?.to_owned()).ok()?;
            native_environment_name_matches(name, &value).then_some(value)
        })
        .collect::<Option<Vec<_>>>()?;
    projected.sort();
    let original_len = projected.len();
    projected.dedup();
    (projected.len() == original_len).then_some(projected)
}

#[cfg(unix)]
fn native_environment_name_matches(original: &std::ffi::OsStr, projected: &EnvironmentName) -> bool {
    use std::os::unix::ffi::OsStrExt as _;

    original.as_bytes() == projected.as_str().as_bytes()
}

#[cfg(windows)]
fn native_environment_name_matches(original: &std::ffi::OsStr, projected: &EnvironmentName) -> bool {
    // `EnvironmentName` accepts ASCII only. ASCII case folding is therefore exactly the
    // case-insensitive ordinal comparison Windows applies to native environment names.
    original
        .to_str()
        .is_some_and(|value| value.eq_ignore_ascii_case(projected.as_str()))
}

fn native_environment_shadows(names: &[OsString]) -> Result<Vec<EnvironmentName>, String> {
    names
        .iter()
        .map(|name| {
            EnvironmentName::new(format!("_PERITUS_NATIVE_{}", native_digest(name)))
                .map_err(|error| format!("construct native environment authority shadow: {error}"))
        })
        .collect()
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
    let output_bytes = resources.output_limit().map(ResourceQuantity::new);
    let limits = TerminalLimits::with_optional_output(size, event_count, output_bytes)
        .map_err(|error| format!("construct command terminal limits: {error}"))?;
    let contract = TerminalContract::new(
        TerminalModes::from_modes([mode]),
        input,
        resize,
        TerminalSignalPermission::Allowed,
        limits,
    )
    .map_err(|error| format!("construct command terminal contract: {error}"))?;
    let requirements = TerminalRequirements::with_optional_output(
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
    ResourceLimits::with_optional_limits(
        resources.wall_millis().map(ResourceQuantity::new),
        resources.cpu_millis().map(ResourceQuantity::new),
        resources.memory_limit().map(ResourceQuantity::new),
        resources.disk_limit().map(ResourceQuantity::new),
        resources.output_limit().map(ResourceQuantity::new),
        resources.file_descriptor_limit().map(ResourceQuantity::new),
        resources.process_limit().map(ResourceQuantity::new),
        Some(ResourceQuantity::new(resources.concurrent_slots())),
    )
    .map_err(|error| format!("construct command resource contract: {error}"))
}

fn native_path(
    value: &std::ffi::OsStr,
    domain: &str,
    context: &'static str,
) -> Result<(SandboxPath, bool), String> {
    if let Some(text) = value.to_str()
        && legacy_path_is_exact(text)
    {
        let normalized = text.replace('\\', "/");
        if let Ok(path) = SandboxPath::new(normalized) {
            return Ok((path, true));
        }
    }
    let path = SandboxPath::new(format!("/__peritus_native/{domain}/{}", native_digest(value)))
        .map_err(|error| format!("{context}: {error}"))?;
    Ok((path, false))
}

const fn legacy_path_is_exact(value: &str) -> bool {
    #[cfg(unix)]
    {
        !value.as_bytes().contains(&b'\\')
    }
    #[cfg(windows)]
    {
        let _ = value;
        true
    }
}

fn native_digest(value: &std::ffi::OsStr) -> String {
    let mut bytes = Vec::new();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;

        bytes.push(1);
        bytes.extend_from_slice(value.as_bytes());
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;

        bytes.push(2);
        for unit in value.encode_wide() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
    }
    let digest = peritus_codec::sha256(&bytes);
    let mut encoded = String::with_capacity(digest.as_bytes().len() * 2);
    for byte in digest.as_bytes() {
        write!(&mut encoded, "{byte:02x}").expect("writing hexadecimal into String cannot fail");
    }
    encoded
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

const fn raw_resource_fidelity() -> ResourceFidelity {
    #[cfg(target_os = "linux")]
    {
        ResourceFidelity::Supervisor
    }
    #[cfg(not(target_os = "linux"))]
    {
        ResourceFidelity::Reference
    }
}

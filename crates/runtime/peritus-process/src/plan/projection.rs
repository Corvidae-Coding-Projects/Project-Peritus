//! Exact checked-sandbox projections into the process execution plan.

use peritus_sandbox::{
    CheckedSandboxPlan, InputPermission, ResizePermission, SandboxResourceKind, TerminalMode,
    TerminalSignalPermission,
};

use crate::{
    CommandSpec, EnvironmentPlan, EnvironmentSource, EnvironmentValueSource, IoMode, OutputPolicy,
    ProcessError, ProcessResourcePolicy, StdinPolicy, TerminalCapabilities, WorkingDirectory,
    error::invalid, native_environment_names_equal,
};

pub(super) fn validate_sandbox_projection(
    sandbox: &CheckedSandboxPlan,
    command: &CommandSpec,
    working_directory: &WorkingDirectory,
    environment: &EnvironmentPlan,
    io_mode: IoMode,
    stdin: StdinPolicy,
    output: OutputPolicy,
    resources: ProcessResourcePolicy,
) -> Result<TerminalCapabilities, ProcessError> {
    let requirements = sandbox.requirements();
    validate_native_execution(sandbox, command, working_directory, environment)?;
    validate_environment(
        environment,
        requirements.environment(),
        sandbox.native_execution().is_some(),
    )?;
    let terminal = requirements.terminal();
    let mode_matches = match (io_mode, terminal.mode()) {
        (IoMode::Pipes, TerminalMode::Pipes) => terminal.initial_size().is_none(),
        (IoMode::Pty(size), TerminalMode::Pty) => terminal.initial_size().is_some_and(|expected| {
            expected.rows() == size.rows() && expected.columns() == size.columns()
        }),
        _ => false,
    };
    let input_matches =
        matches!(stdin, StdinPolicy::Closed) == matches!(terminal.input(), InputPermission::Denied);
    let resize_allowed = matches!(terminal.resize(), ResizePermission::Allowed);
    let signals_allowed = matches!(terminal.signals(), TerminalSignalPermission::Allowed);
    let event_count = terminal.event_count().get();
    let output_bytes = terminal.output_limit().map(peritus_types::ResourceQuantity::get);
    let output_matches = output.event_count() <= event_count
        && allowance_within(output.spool_limit(), output_bytes)
        && match io_mode {
            IoMode::Pipes => allowance_within(output.stdout_limit(), output_bytes)
                && allowance_within(output.stderr_limit(), output_bytes),
            IoMode::Pty(_) => allowance_within(output.terminal_limit(), output_bytes),
        };
    if !mode_matches || !input_matches || !output_matches {
        return Err(invalid("process I/O differs from checked sandbox requirements"));
    }
    validate_resources(resources, requirements.resources())?;
    Ok(TerminalCapabilities::new(resize_allowed, signals_allowed, event_count, output_bytes))
}

fn validate_environment(
    environment: &EnvironmentPlan,
    requirements: &peritus_sandbox::EnvironmentRequirements,
    native_authority: bool,
) -> Result<(), ProcessError> {
    if native_authority {
        return Ok(());
    }
    let source_matches = match environment.source() {
        EnvironmentSource::Cleared => requirements.inherited_names().is_empty(),
        EnvironmentSource::Allowlisted(names) => {
            names.len() == requirements.inherited_names().len()
                && names
                    .iter()
                    .zip(requirements.inherited_names())
                    .all(|(left, right)| {
                        left.to_str()
                            .is_some_and(|left| left.eq_ignore_ascii_case(right.as_str()))
                    })
        }
    };
    let names_allowed = environment.variables().iter().all(|variable| {
        let allowed = match variable.source() {
            EnvironmentValueSource::Inherited => requirements.inherited_names(),
            EnvironmentValueSource::Literal => requirements.literal_names(),
        };
        allowed.iter().any(|name| {
            variable
                .name()
                .to_str()
                .is_some_and(|variable| variable.eq_ignore_ascii_case(name.as_str()))
        })
    });
    if !source_matches || !names_allowed {
        return Err(invalid("process environment differs from checked sandbox requirements"));
    }
    Ok(())
}

fn validate_native_execution(
    sandbox: &CheckedSandboxPlan,
    command: &CommandSpec,
    working_directory: &WorkingDirectory,
    environment: &EnvironmentPlan,
) -> Result<(), ProcessError> {
    let Some(authority) = sandbox.native_execution() else {
        let legacy_program = command.executable().to_str().is_some_and(|executable| {
            executable == sandbox.requirements().process().program().as_str()
        });
        if !command.uses_legacy_encoding()
            || !working_directory.uses_legacy_encoding()
            || !environment.uses_legacy_encoding()
            || !legacy_program
        {
            return Err(invalid(
                "native command, directory, or environment is absent from sandbox authority",
            ));
        }
        return Ok(());
    };
    let inherited = match environment.source() {
        EnvironmentSource::Cleared => &[][..],
        EnvironmentSource::Allowlisted(names) => names.as_slice(),
    };
    let literals = environment
        .variables()
        .iter()
        .filter(|variable| variable.source() == EnvironmentValueSource::Literal)
        .map(|variable| variable.name())
        .collect::<Vec<_>>();
    let inherited_exact = inherited.len() == authority.inherited_environment().len()
        && inherited
            .iter()
            .zip(authority.inherited_environment())
            .all(|(left, right)| native_environment_names_equal(left, right));
    let literals_exact = literals.len() == authority.literal_environment().len()
        && literals
            .iter()
            .zip(authority.literal_environment())
            .all(|(left, right)| native_environment_names_equal(left, right));
    if command.executable() != authority.executable()
        || working_directory.path().as_os_str()
            != authority.working_directory().as_os_str()
        || !inherited_exact
        || !literals_exact
    {
        return Err(invalid("execution differs from exact native sandbox authority"));
    }
    Ok(())
}

fn validate_resources(
    resources: ProcessResourcePolicy,
    expected: &peritus_sandbox::ResourceLimits,
) -> Result<(), ProcessError> {
    let matches = optional_quantity(expected, SandboxResourceKind::WallTime)
        == resources.wall_millis()
        && optional_quantity(expected, SandboxResourceKind::CpuTime) == resources.cpu_millis()
        && optional_quantity(expected, SandboxResourceKind::Memory) == resources.memory_limit()
        && optional_quantity(expected, SandboxResourceKind::Disk) == resources.disk_limit()
        && optional_quantity(expected, SandboxResourceKind::Output) == resources.output_limit()
        && optional_quantity(expected, SandboxResourceKind::Processes) == resources.process_limit()
        && optional_quantity(expected, SandboxResourceKind::OpenHandles)
            == resources.file_descriptor_limit()
        && optional_quantity(expected, SandboxResourceKind::Concurrency)
            == Some(resources.concurrent_slots());
    if !matches {
        return Err(invalid("process resources differ from checked sandbox requirements"));
    }
    Ok(())
}

const fn optional_quantity(
    limits: &peritus_sandbox::ResourceLimits,
    kind: SandboxResourceKind,
) -> Option<u64> {
    match limits.selected_limit(kind) {
        Some(value) => Some(value.get()),
        None => None,
    }
}

const fn allowance_within(value: Option<u64>, maximum: Option<u64>) -> bool {
    match (value, maximum) {
        (_, None) => true,
        (Some(value), Some(maximum)) => value <= maximum,
        (None, Some(_)) => false,
    }
}

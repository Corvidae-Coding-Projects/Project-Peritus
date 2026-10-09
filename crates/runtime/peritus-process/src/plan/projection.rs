//! Exact checked-sandbox projections into the process execution plan.

use peritus_sandbox::{
    CheckedSandboxPlan, InputPermission, ResizePermission, SandboxResourceKind, TerminalMode,
    TerminalSignalPermission,
};

use crate::{
    EnvironmentPlan, EnvironmentSource, EnvironmentValueSource, IoMode, OutputPolicy, ProcessError,
    ProcessResourcePolicy, StdinPolicy, TerminalCapabilities, error::invalid,
};

pub(super) fn validate_sandbox_projection(
    sandbox: &CheckedSandboxPlan,
    environment: &EnvironmentPlan,
    io_mode: IoMode,
    stdin: StdinPolicy,
    output: OutputPolicy,
    resources: ProcessResourcePolicy,
) -> Result<TerminalCapabilities, ProcessError> {
    let requirements = sandbox.requirements();
    validate_environment(environment, requirements.environment())?;
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
    let output_bytes = terminal.output_bytes().get();
    let output_matches = output.event_count() <= event_count
        && output.spool_bytes() <= output_bytes
        && match io_mode {
            IoMode::Pipes => {
                output.stdout_bytes() <= output_bytes && output.stderr_bytes() <= output_bytes
            }
            IoMode::Pty(_) => output.terminal_bytes() <= output_bytes,
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
) -> Result<(), ProcessError> {
    let source_matches = match environment.source() {
        EnvironmentSource::Cleared => requirements.inherited_names().is_empty(),
        EnvironmentSource::Allowlisted(names) => {
            names.len() == requirements.inherited_names().len()
                && names
                    .iter()
                    .zip(requirements.inherited_names())
                    .all(|(left, right)| left.eq_ignore_ascii_case(right.as_str()))
        }
    };
    let names_allowed = environment.variables().iter().all(|variable| {
        let allowed = match variable.source() {
            EnvironmentValueSource::Inherited => requirements.inherited_names(),
            EnvironmentValueSource::Literal => requirements.literal_names(),
        };
        allowed.iter().any(|name| variable.name().eq_ignore_ascii_case(name.as_str()))
    });
    if !source_matches || !names_allowed {
        return Err(invalid("process environment differs from checked sandbox requirements"));
    }
    Ok(())
}

const fn validate_resources(
    resources: ProcessResourcePolicy,
    expected: &peritus_sandbox::ResourceLimits,
) -> Result<(), ProcessError> {
    let matches = expected.limit(SandboxResourceKind::WallTime).get() == resources.wall_millis()
        && expected.limit(SandboxResourceKind::CpuTime).get() == resources.cpu_millis()
        && expected.limit(SandboxResourceKind::Memory).get() == resources.memory_bytes()
        && expected.limit(SandboxResourceKind::Disk).get() == resources.disk_bytes()
        && expected.limit(SandboxResourceKind::Output).get() == resources.output_bytes()
        && expected.limit(SandboxResourceKind::Processes).get() == resources.process_count()
        && expected.limit(SandboxResourceKind::OpenHandles).get() == resources.file_descriptors()
        && expected.limit(SandboxResourceKind::Concurrency).get() == resources.concurrent_slots();
    if !matches {
        return Err(invalid("process resources differ from checked sandbox requirements"));
    }
    Ok(())
}

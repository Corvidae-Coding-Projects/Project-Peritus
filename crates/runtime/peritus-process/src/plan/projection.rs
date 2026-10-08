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
        && output.spool_bytes().is_none_or(|limit| limit <= output_bytes)
        && match io_mode {
            IoMode::Pipes => {
                output.stdout_bytes().is_none_or(|limit| limit <= output_bytes)
                    && output.stderr_bytes().is_none_or(|limit| limit <= output_bytes)
            }
            IoMode::Pty(_) => output.terminal_bytes().is_none_or(|limit| limit <= output_bytes),
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
            // Native environments retain platform ordering; sandbox names use uppercase ASCII.
            // Compare canonical identities, not positions in differently ordered sequences.
            let canonical = names
                .iter()
                .map(|name| name.to_ascii_uppercase())
                .collect::<std::collections::BTreeSet<_>>();
            canonical.len() == names.len()
                && canonical.len() == requirements.inherited_names().len()
                && canonical
                    .iter()
                    .zip(requirements.inherited_names())
                    .all(|(left, right)| left == right.as_str())
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

fn validate_resources(
    resources: ProcessResourcePolicy,
    expected: &peritus_sandbox::ResourceLimits,
) -> Result<(), ProcessError> {
    let matches = expected.wall_time_limit()
        == resources.wall_millis().map(peritus_types::ResourceQuantity::new)
        && expected.cpu_time_limit().map(peritus_types::ResourceQuantity::get)
            == resources.cpu_millis()
        && expected.limit(SandboxResourceKind::Memory).get() == resources.memory_bytes()
        && expected.limit(SandboxResourceKind::Disk).get() == resources.disk_bytes()
        && expected.output_limit()
            == resources.output_bytes().map(peritus_types::ResourceQuantity::new)
        && expected.limit(SandboxResourceKind::Processes).get() == resources.process_count()
        && expected.limit(SandboxResourceKind::OpenHandles).get() == resources.file_descriptors()
        && expected.limit(SandboxResourceKind::Concurrency).get() == resources.concurrent_slots();
    if !matches {
        return Err(invalid("process resources differ from checked sandbox requirements"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_environment;
    use crate::EnvironmentPlan;
    use peritus_sandbox::{EnvironmentName, EnvironmentRequirements};

    #[test]
    fn environment_projection_compares_canonical_names_independent_of_native_order() {
        let names = vec!["Z_PERITUS_PROJECTION".to_owned(), "a_peritus_projection".to_owned()];
        let environment = EnvironmentPlan::allowlisted(names.clone(), Vec::new()).expect("plan");
        let requirements = EnvironmentRequirements::new(
            names.into_iter().map(|name| EnvironmentName::new(name).expect("name")).collect(),
            Vec::new(),
        )
        .expect("requirements");
        validate_environment(&environment, &requirements).expect("equal canonical names");
        let wrong = EnvironmentRequirements::new(
            vec![EnvironmentName::new("OTHER").expect("name")],
            Vec::new(),
        )
        .expect("wrong requirements");
        assert!(validate_environment(&environment, &wrong).is_err());
    }

    #[cfg(not(windows))]
    #[test]
    fn environment_projection_rejects_ambiguous_case_folded_inheritance() {
        let environment = EnvironmentPlan::allowlisted(
            vec!["PERITUS_PROJECTION".to_owned(), "peritus_projection".to_owned()],
            Vec::new(),
        )
        .expect("distinct native names");
        let requirements = EnvironmentRequirements::new(
            vec![EnvironmentName::new("PERITUS_PROJECTION").expect("name")],
            Vec::new(),
        )
        .expect("requirements");
        assert!(validate_environment(&environment, &requirements).is_err());
    }
}

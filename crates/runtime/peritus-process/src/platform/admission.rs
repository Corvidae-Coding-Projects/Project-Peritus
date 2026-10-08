//! Side-effect-free current-platform argv and environment admission.

use std::mem::size_of;

use crate::{
    CommandSpec, EnvironmentPlan, ErrorCode, ExecutionPlan, ProcessError, ProcessOperation,
    RecoveryClass,
};

pub(super) fn validate(plan: &ExecutionPlan) -> Result<(), ProcessError> {
    validate_command_environment(plan.command(), plan.environment())
}

/// Validates exact native argv and environment values against current operating-system capacity.
///
/// Native helpers repeat this check immediately before target creation so an advisory capacity
/// change between C2 admission and C3 consumption fails against the original admitted intent.
///
/// # Errors
/// Returns a typed validation error when current native capacity cannot be queried or is exceeded.
pub fn validate_command_environment(
    command: &CommandSpec,
    environment: &EnvironmentPlan,
) -> Result<(), ProcessError> {
    validate_inner(command, environment)
}

#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "sysconf is the POSIX boundary for live advisory exec capacity"
)]
fn validate_inner(
    command: &CommandSpec,
    environment: &EnvironmentPlan,
) -> Result<(), ProcessError> {
    // SAFETY: sysconf has no pointer arguments or caller-owned memory.
    let arg_max = unsafe { libc::sysconf(libc::_SC_ARG_MAX) };
    if arg_max <= 0 {
        return Err(capacity_unavailable("native ARG_MAX cannot be queried"));
    }
    let arg_max = usize::try_from(arg_max)
        .map_err(|_| capacity_unavailable("native ARG_MAX cannot be represented"))?;
    let mut string_bytes = checked_add(
        crate::command::native_len(command.executable()),
        1,
        "native argv accounting overflowed",
    )?;
    validate_unix_string(string_bytes)?;
    for argument in command.arguments() {
        let bytes = checked_add(
            crate::command::native_len(argument),
            1,
            "native argv accounting overflowed",
        )?;
        validate_unix_string(bytes)?;
        string_bytes = checked_add(string_bytes, bytes, "native argv accounting overflowed")?;
    }
    for variable in environment.variables() {
        let bytes = crate::command::native_len(variable.name())
            .checked_add(crate::command::native_len(variable.value()))
            .and_then(|value| value.checked_add(2))
            .ok_or_else(|| capacity_exceeded("native environment accounting overflowed"))?;
        validate_unix_string(bytes)?;
        string_bytes =
            checked_add(string_bytes, bytes, "native environment accounting overflowed")?;
    }
    let pointers = command
        .arguments()
        .len()
        .checked_add(environment.variables().len())
        .and_then(|value| value.checked_add(3))
        .and_then(|value| value.checked_mul(size_of::<usize>()))
        .ok_or_else(|| capacity_exceeded("native exec pointer accounting overflowed"))?;
    let total = checked_add(string_bytes, pointers, "native exec accounting overflowed")?;
    if total > arg_max {
        return Err(capacity_exceeded(
            "native argv and environment exceed the live advisory ARG_MAX",
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
#[allow(
    unsafe_code,
    reason = "sysconf is the Linux boundary for the live page-sized per-string exec constraint"
)]
fn validate_unix_string(bytes_with_nul: usize) -> Result<(), ProcessError> {
    // SAFETY: sysconf has no pointer arguments or caller-owned memory.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page_size <= 0 {
        return Err(capacity_unavailable("native page size cannot be queried"));
    }
    let maximum = usize::try_from(page_size)
        .ok()
        .and_then(|value| value.checked_mul(32))
        .ok_or_else(|| capacity_unavailable("native per-string capacity cannot be represented"))?;
    if bytes_with_nul > maximum {
        return Err(capacity_exceeded(
            "one native argv or environment string exceeds the Linux exec capacity",
        ));
    }
    Ok(())
}

#[cfg(all(unix, not(target_os = "linux")))]
const fn validate_unix_string(_bytes_with_nul: usize) -> Result<(), ProcessError> {
    Ok(())
}

#[cfg(windows)]
fn validate_inner(
    command: &CommandSpec,
    environment: &EnvironmentPlan,
) -> Result<(), ProcessError> {
    let _command_line = command.windows_command_line()?;
    let mut environment_units = usize::from(environment.variables().is_empty()) + 1;
    for variable in environment.variables() {
        environment_units = environment_units
            .checked_add(crate::command::native_len(variable.name()))
            .and_then(|value| value.checked_add(crate::command::native_len(variable.value())))
            .and_then(|value| value.checked_add(2))
            .ok_or_else(|| capacity_exceeded("native Windows environment allocation overflowed"))?;
    }
    let _environment_bytes = environment_units
        .checked_mul(size_of::<u16>())
        .ok_or_else(|| capacity_exceeded("native Windows environment allocation overflowed"))?;
    Ok(())
}

fn checked_add(
    left: usize,
    right: usize,
    detail: &'static str,
) -> Result<usize, ProcessError> {
    left.checked_add(right).ok_or_else(|| capacity_exceeded(detail))
}

const fn capacity_exceeded(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::InvalidInput,
        ProcessOperation::Validate,
        RecoveryClass::CorrectRequest,
        detail,
    )
}

const fn capacity_unavailable(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Unsupported,
        ProcessOperation::Validate,
        RecoveryClass::SelectBackend,
        detail,
    )
}

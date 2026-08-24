//! Process-command projection helpers.

use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery, TargetCommand};
use peritus_process::ExecutionPlan;
use peritus_sandbox::CheckedSandboxPlan;

pub fn target_command(
    execution: &ExecutionPlan,
    sandbox: &CheckedSandboxPlan,
) -> Result<TargetCommand, LinuxError> {
    if execution.command().executable() != sandbox.requirements().process().program().as_str() {
        return Err(LinuxError::new(
            LinuxErrorKind::PreparationMismatch,
            LinuxOperation::Prepare,
            LinuxRecovery::Replan,
            "execution command differs from the checked sandbox root program",
        ));
    }
    TargetCommand::new(
        execution.command().executable().to_owned(),
        execution.command().arguments().to_vec(),
    )
}

pub fn environment(execution: &ExecutionPlan) -> Result<Vec<crate::EnvironmentEntry>, LinuxError> {
    #[cfg(unix)]
    if execution
        .environment()
        .variables()
        .iter()
        .any(|variable| variable.name() == peritus_process::NATIVE_PTY_SLAVE_ENV)
    {
        return Err(LinuxError::new(
            LinuxErrorKind::PreparationMismatch,
            LinuxOperation::Prepare,
            LinuxRecovery::CorrectRequest,
            "execution environment uses the reserved native PTY attachment key",
        ));
    }
    execution
        .environment()
        .variables()
        .iter()
        .map(|variable| {
            crate::EnvironmentEntry::new(variable.name().to_owned(), variable.value().to_owned())
        })
        .collect()
}

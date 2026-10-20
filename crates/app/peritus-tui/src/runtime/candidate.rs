//! Foreground execution of a selected candidate's documented run command.

use std::{path::PathBuf, process::Command};

use peritus_tools_shell::ExecInput;
use peritus_types::Sha256Digest;

#[cfg(unix)]
mod foreground;

/// Runs the candidate while the full-screen terminal is suspended.
pub(super) async fn execute(
    workspace: PathBuf,
    instruction: String,
    candidate_digest: Sha256Digest,
    interrupts: &mut super::interrupts::Interrupts,
) -> Result<(), String> {
    let mut task = tokio::task::spawn_blocking(move || {
        execute_blocking(&workspace, &instruction, candidate_digest)
    });
    loop {
        tokio::select! {
            biased;
            _ = interrupts.recv() => {
                // Keep owning the foreground command until it exits, then restore the UI.
            }
            result = &mut task => {
                super::interrupts::drain(interrupts);
                return result.map_err(|error| format!("candidate command task failed: {error}"))?;
            }
        }
    }
}

fn execute_blocking(
    workspace: &PathBuf,
    instruction: &str,
    candidate_digest: Sha256Digest,
) -> Result<(), String> {
    let current = peritus_workspace::task_candidate::identity(workspace)
        .map_err(|error| format!("could not validate exact candidate: {}", error.detail()))?;
    if current != candidate_digest {
        return Err(
            "candidate changed after settlement; continue the run to inspect and requalify it"
                .to_owned(),
        );
    }
    let mut command = direct_command(instruction)?;
    command.current_dir(workspace);
    #[cfg(unix)]
    let status = foreground::status(&mut command);
    #[cfg(not(unix))]
    let status = command.status();
    let status = status.map_err(|error| format!("could not run candidate command: {error}"))?;
    if status.success() { Ok(()) } else { Err(format!("candidate command exited with {status}")) }
}

fn direct_command(instruction: &str) -> Result<Command, String> {
    let input = ExecInput::from_command_line(instruction).map_err(|error| {
        format!("candidate run instruction is not executable: {}", error.detail())
    })?;
    let mut command = Command::new(input.executable());
    command.args(input.arguments());
    Ok(command)
}

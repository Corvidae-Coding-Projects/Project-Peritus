//! Foreground execution of a selected candidate's documented run command.

use std::{path::PathBuf, process::Command};

use peritus_product_runner::ProductRunner;
use peritus_tools_shell::ExecInput;
use peritus_types::Sha256Digest;

/// Runs the candidate while the full-screen terminal is suspended.
pub(super) async fn execute(
    workspace: PathBuf,
    instruction: String,
    candidate_digest: Sha256Digest,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        execute_blocking(&workspace, &instruction, candidate_digest)
    })
    .await
    .map_err(|error| format!("candidate command task failed: {error}"))?
}

fn execute_blocking(
    workspace: &PathBuf,
    instruction: &str,
    candidate_digest: Sha256Digest,
) -> Result<(), String> {
    let current = ProductRunner::candidate_digest(workspace)
        .map_err(|error| format!("could not validate exact candidate: {}", error.detail()))?;
    if current != candidate_digest {
        return Err(
            "candidate changed after settlement; continue the run to inspect and requalify it"
                .to_owned(),
        );
    }
    let mut command = direct_command(instruction)?;
    let status = command
        .current_dir(workspace)
        .status()
        .map_err(|error| format!("could not start candidate command: {error}"))?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn reports_command_success_and_failure() {
        let workspace = tempfile::tempdir().expect("workspace");
        run(workspace.path(), &["init", "--quiet"]);
        run(workspace.path(), &["config", "user.name", "Peritus Test"]);
        run(workspace.path(), &["config", "user.email", "peritus@example.invalid"]);
        run(workspace.path(), &["commit", "--quiet", "--allow-empty", "-m", "initial"]);
        let digest = ProductRunner::candidate_digest(workspace.path()).expect("candidate digest");
        assert!(
            execute(workspace.path().to_path_buf(), success_command().to_owned(), digest)
                .await
                .is_ok()
        );
        assert!(
            execute(workspace.path().to_path_buf(), failure_command().to_owned(), digest)
                .await
                .is_err()
        );
        assert!(
            execute(
                workspace.path().to_path_buf(),
                "rustc --version && rustc --version".to_owned(),
                digest,
            )
            .await
            .is_err()
        );
        std::fs::write(workspace.path().join("changed.txt"), "changed").expect("candidate change");
        let error = execute(workspace.path().to_path_buf(), success_command().to_owned(), digest)
            .await
            .expect_err("changed candidate");
        assert!(error.contains("candidate changed after settlement"));
    }

    fn run(root: &std::path::Path, arguments: &[&str]) {
        let status =
            Command::new("git").args(arguments).current_dir(root).status().expect("git fixture");
        assert!(status.success());
    }

    const fn success_command() -> &'static str {
        "rustc --version"
    }

    const fn failure_command() -> &'static str {
        "rustc --definitely-invalid-peritus-option"
    }
}

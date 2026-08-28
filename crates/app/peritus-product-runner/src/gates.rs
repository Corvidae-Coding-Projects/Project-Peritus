//! Native project-gate discovery and execution.

use std::{fmt::Write as _, path::Path, process::Command};

use crate::{ProductRunnerError, ProductRunnerErrorKind, bundle::limit_text};

pub struct GateReport {
    pub passed: bool,
    pub output: String,
}

pub fn run(root: &Path) -> Result<GateReport, ProductRunnerError> {
    let commands = discover(root);
    if commands.is_empty() {
        return Ok(GateReport {
            passed: true,
            output: "No standard project test command was detected.".to_owned(),
        });
    }
    let mut passed = true;
    let mut report = String::new();
    for (program, arguments) in commands {
        let mut command = Command::new(program);
        command.args(arguments).current_dir(root);
        if program == "cargo" {
            command.env("CARGO_BUILD_JOBS", "2");
        }
        let output = command.output().map_err(|error| {
            ProductRunnerError::new(
                ProductRunnerErrorKind::Gate,
                "run repository gate",
                error.to_string(),
            )
        })?;
        passed &= output.status.success();
        report.push_str("$ ");
        report.push_str(program);
        for argument in arguments {
            report.push(' ');
            report.push_str(argument);
        }
        report.push('\n');
        report.push_str(&String::from_utf8_lossy(&output.stdout));
        report.push_str(&String::from_utf8_lossy(&output.stderr));
        let _ = write!(report, "\nexit: {}\n\n", output.status);
    }
    Ok(GateReport { passed, output: limit_text(&report, 1024 * 1024) })
}

fn discover(root: &Path) -> Vec<(&'static str, &'static [&'static str])> {
    let mut commands = Vec::new();
    if root.join("Cargo.toml").is_file() {
        commands.push(("cargo", &["test", "--all-targets"] as &[_]));
    }
    if root.join("package.json").is_file() {
        commands.push(("npm", &["test"] as &[_]));
    }
    if root.join("pyproject.toml").is_file() || root.join("pytest.ini").is_file() {
        commands.push(("python", &["-m", "pytest"] as &[_]));
    }
    if root.join("go.mod").is_file() {
        commands.push(("go", &["test", "./..."] as &[_]));
    }
    commands
}

use crate::error::{Diagnostic, ErrorCode, XtaskError};
use crate::model::ToolchainPolicy;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

pub(crate) fn check(root: &Path, policy: &ToolchainPolicy) -> Result<(), XtaskError> {
    let mut diagnostics = Vec::new();
    let rustc = run(root, "rustc", &["--version"])?;
    expect_output(
        &rustc,
        &format!("rustc {} ", policy.rust),
        "rustc --version",
        "install the exact rust-toolchain.toml toolchain",
        &mut diagnostics,
    );

    let verus_path = executable("verus").ok_or_else(|| {
        XtaskError::violations(
            ErrorCode::Reproducibility,
            "toolchain-check",
            vec![Diagnostic::new(
                "Verus executable is not available on PATH",
                "install the pinned archive from toolchains.toml and add its directory to PATH",
            )],
        )
    })?;
    let verus = Command::new(&verus_path)
        .arg("--version")
        .current_dir(root)
        .output()
        .map_err(|error| XtaskError::io("execute", &verus_path, error))?;
    expect_output(
        &verus,
        &format!("Version: {}", policy.verus),
        "verus --version",
        "install the exact Verus version pinned in toolchains.toml",
        &mut diagnostics,
    );

    let canonical_verus = fs::canonicalize(&verus_path)
        .map_err(|error| XtaskError::io("canonicalize", &verus_path, error))?;
    let solver_name = if cfg!(windows) { "z3.exe" } else { "z3" };
    let solver_path = canonical_verus.parent().unwrap_or_else(|| Path::new(".")).join(solver_name);
    let solver = Command::new(&solver_path)
        .arg("--version")
        .current_dir(root)
        .output()
        .map_err(|error| XtaskError::io("execute", &solver_path, error))?;
    expect_output(
        &solver,
        &format!("Z3 version {} ", policy.z3),
        "bundled z3 --version",
        "use the solver shipped beside the pinned Verus executable",
        &mut diagnostics,
    );

    let advertised = run(root, "cargo", &["verus", "toolchain", "list"])?;
    for (needle, description) in [
        (format!("verus = \"{}\"", policy.verus), "Verus release"),
        (format!("rev = \"{}\"", policy.vstd_revision), "vstd revision"),
        (
            format!("z3 = \"{}\"", policy.cargo_verus_advertised_z3),
            "cargo-verus advertised solver metadata",
        ),
    ] {
        expect_output(
            &advertised,
            &needle,
            "cargo verus toolchain list",
            &format!("install cargo-verus from the pinned Verus archive; expected {description}"),
            &mut diagnostics,
        );
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(XtaskError::violations(ErrorCode::Reproducibility, "toolchain-check", diagnostics))
    }
}

fn run(root: &Path, program: &str, args: &[&str]) -> Result<Output, XtaskError> {
    Command::new(program)
        .args(args)
        .current_dir(root)
        .output()
        .map_err(|error| XtaskError::io("execute", Path::new(program), error))
}

fn executable(name: &str) -> Option<PathBuf> {
    let executable_name = if cfg!(windows) { format!("{name}.exe") } else { name.to_owned() };
    env::split_paths(&env::var_os("PATH")?)
        .map(|directory| directory.join(&executable_name))
        .find(|candidate| candidate.is_file())
}

fn expect_output(
    output: &Output,
    expected: &str,
    command: &str,
    help: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let stdout = String::from_utf8_lossy(&output.stdout);
    if !output.status.success() || !stdout.contains(expected) {
        diagnostics.push(Diagnostic::new(
            format!(
                "`{command}` did not report `{expected}` (status {}, stdout `{}`)",
                output.status,
                stdout.trim()
            ),
            help,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::executable;

    #[test]
    fn finds_the_active_rust_compiler() {
        assert!(executable("rustc").is_some());
    }
}

use crate::api_contract;
use crate::architecture;
use crate::error::XtaskError;
use crate::metadata;
use crate::reproducibility;
use crate::source;
use crate::toolchain;
use crate::trust;
use std::env;
use std::ffi::OsString;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const HELP: &str = "Peritus workspace policy tool

Usage: cargo xtask <command>

Commands:
  all                    Run all repository-only policy checks
  architecture-check     Validate packages, layers, ownership, and source layout
  ordinary-api-check     Validate formal APIs callable from ordinary safe Rust
  source-layout-check    Validate module names, crate roots, and source budgets
  reproducibility-check  Validate toolchain pins, lock policy, and immutable CI inputs
  toolchain-check        Probe installed Rust, Verus, vstd metadata, and bundled Z3
  verify-trust           Reject trusted Verus constructs outside approved roots
  help                   Print this help
";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Command {
    All,
    Architecture,
    OrdinaryApi,
    SourceLayout,
    Reproducibility,
    Toolchain,
    Trust,
    Help,
}

/// Runs `xtask` using process arguments, the current directory, and standard output.
///
/// # Errors
///
/// Returns a typed error when invocation, filesystem access, Cargo metadata, or a policy check
/// fails. Diagnostics include a stable category and recovery guidance.
pub fn run_from_env() -> Result<(), XtaskError> {
    let current = env::current_dir().map_err(|error| {
        XtaskError::io("determine current directory from", Path::new("."), error)
    })?;
    let root = discover_workspace_root(&current)?;
    execute(env::args_os().skip(1), &root, &mut io::stdout().lock())
}

fn discover_workspace_root(start: &Path) -> Result<PathBuf, XtaskError> {
    let mut candidate = fs::canonicalize(start)
        .map_err(|error| XtaskError::io("canonicalize workspace search path", start, error))?;
    loop {
        if candidate.join("Cargo.toml").is_file() && candidate.join("architecture.toml").is_file() {
            return Ok(candidate);
        }
        if !candidate.pop() {
            return Err(XtaskError::metadata(format!(
                "could not locate the Peritus workspace above {}",
                start.display()
            )));
        }
    }
}

pub(crate) fn execute(
    args: impl IntoIterator<Item = OsString>,
    root: &Path,
    output: &mut dyn Write,
) -> Result<(), XtaskError> {
    let command = parse(args)?;
    if command == Command::Help {
        write_output(output, HELP)?;
        return Ok(());
    }

    match command {
        Command::All => {
            let policy = metadata::architecture_policy(root)?;
            let (packages, files) = architecture::check(root, &policy)?;
            let api = api_contract::check(root, &policy)?;
            let trust_files = trust::check(root, &policy)?;
            let tools = metadata::toolchain_policy(root)?;
            let actions = reproducibility::check(root, &tools)?;
            write_output(
                output,
                &format!(
                    "all checks passed: {packages} package(s), {files} source file(s), \
                     {} formal-boundary file(s), {} ordinary-safe executable entry point(s), \
                     {trust_files} trust-scanned file(s), {actions} pinned action(s)\n",
                    api.files, api.executable_entry_points
                ),
            )?;
        }
        Command::Architecture => {
            let policy = metadata::architecture_policy(root)?;
            let (packages, files) = architecture::check(root, &policy)?;
            write_output(
                output,
                &format!(
                    "architecture-check passed: {packages} package(s), {files} source file(s)\n"
                ),
            )?;
        }
        Command::OrdinaryApi => {
            let policy = metadata::architecture_policy(root)?;
            let report = api_contract::check(root, &policy)?;
            write_output(
                output,
                &format!(
                    "ordinary-api-check passed: {} formal-boundary file(s), {} ordinary-safe executable entry point(s)\n",
                    report.files, report.executable_entry_points
                ),
            )?;
        }
        Command::SourceLayout => {
            let policy = metadata::architecture_policy(root)?;
            let cargo = metadata::cargo_metadata(root)?;
            let files = source::check(root, &policy, &cargo)?;
            write_output(output, &format!("source-layout-check passed: {files} source file(s)\n"))?;
        }
        Command::Reproducibility => {
            let tools = metadata::toolchain_policy(root)?;
            let actions = reproducibility::check(root, &tools)?;
            write_output(
                output,
                &format!("reproducibility-check passed: {actions} immutable action reference(s)\n"),
            )?;
        }
        Command::Toolchain => {
            let tools = metadata::toolchain_policy(root)?;
            toolchain::check(root, &tools)?;
            write_output(
                output,
                "toolchain-check passed: Rust, Verus, vstd metadata, and bundled Z3 match\n",
            )?;
        }
        Command::Trust => {
            let policy = metadata::architecture_policy(root)?;
            let files = trust::check(root, &policy)?;
            write_output(
                output,
                &format!("verify-trust passed: {files} source file(s) scanned\n"),
            )?;
        }
        Command::Help => {}
    }
    Ok(())
}

fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Command, XtaskError> {
    let mut args = args.into_iter();
    let first = args.next();
    if args.next().is_some() {
        return Err(XtaskError::invocation(
            "expected exactly one command; run `cargo xtask help` for the supported interface",
        ));
    }
    match first.as_deref().and_then(|value| value.to_str()) {
        Some("all") => Ok(Command::All),
        Some("architecture-check") => Ok(Command::Architecture),
        Some("ordinary-api-check") => Ok(Command::OrdinaryApi),
        Some("source-layout-check") => Ok(Command::SourceLayout),
        Some("reproducibility-check") => Ok(Command::Reproducibility),
        Some("toolchain-check") => Ok(Command::Toolchain),
        Some("verify-trust") => Ok(Command::Trust),
        Some("help" | "-h" | "--help") | None => Ok(Command::Help),
        Some(command) => Err(XtaskError::invocation(format!(
            "unknown command `{command}`; run `cargo xtask help` for the supported interface"
        ))),
    }
}

fn write_output(output: &mut dyn Write, message: &str) -> Result<(), XtaskError> {
    output
        .write_all(message.as_bytes())
        .map_err(|error| XtaskError::io("write", Path::new("<stdout>"), error))
}

#[cfg(test)]
mod tests {
    use super::{Command, discover_workspace_root, parse};
    use crate::error::ErrorCode;
    use std::ffi::OsString;
    use std::path::Path;

    #[test]
    fn empty_arguments_show_help() {
        assert_eq!(parse(Vec::<OsString>::new()).expect("empty args are valid"), Command::Help);
    }

    #[test]
    fn unknown_command_has_stable_typed_error() {
        let error = parse([OsString::from("unknown")]).expect_err("unknown command must fail");
        assert_eq!(error.code(), ErrorCode::Invocation);
        assert!(error.render().contains("cargo xtask help"));
    }

    #[test]
    fn workspace_root_is_discovered_from_the_xtask_directory() {
        let crate_root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace = discover_workspace_root(crate_root)
            .expect("xtask must be nested under the Peritus workspace root");
        assert_eq!(workspace.join("xtask"), crate_root);
        assert!(workspace.join("architecture.toml").is_file());
    }
}

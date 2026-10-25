//! Dispatch for the retained package, installation, distribution, and release tools.

use crate::error::XtaskError;
use std::{
    env,
    ffi::OsString,
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

mod help;

/// Runs the packaging tool with process arguments.
///
/// # Errors
/// Returns an invocation, filesystem, package build, or release operation error.
pub fn run_from_env() -> Result<(), XtaskError> {
    let current = env::current_dir().map_err(|error| {
        XtaskError::io("determine current directory from", Path::new("."), error)
    })?;
    let root = discover_workspace_root(&current)?;
    execute(env::args_os().skip(1), &root, &mut io::stdout().lock())
}

fn discover_workspace_root(start: &Path) -> Result<PathBuf, XtaskError> {
    let canonical = fs::canonicalize(start)
        .map_err(|error| XtaskError::io("canonicalize workspace search path", start, error))?;
    let mut candidate = child_process_path(canonical);
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

#[cfg(not(windows))]
const fn child_process_path(path: PathBuf) -> PathBuf {
    path
}

#[cfg(windows)]
fn child_process_path(path: PathBuf) -> PathBuf {
    use std::path::{Component, Prefix};

    let mut components = path.components();
    let Some(Component::Prefix(prefix)) = components.next() else {
        return path;
    };
    let mut ordinary = match prefix.kind() {
        Prefix::VerbatimDisk(drive) => PathBuf::from(format!("{}:\\", char::from(drive))),
        Prefix::VerbatimUNC(server, share) => {
            let mut value = OsString::from(r"\\");
            value.push(server);
            value.push(r"\");
            value.push(share);
            PathBuf::from(value)
        }
        _ => return path,
    };
    for component in components {
        if !matches!(component, Component::RootDir) {
            ordinary.push(component.as_os_str());
        }
    }
    ordinary
}

pub fn execute(
    args: impl IntoIterator<Item = OsString>,
    root: &Path,
    output: &mut dyn Write,
) -> Result<(), XtaskError> {
    let mut args = args.into_iter();
    let first = args.next();
    if args.next().is_some() {
        return Err(XtaskError::invocation("expected one command; run cargo xtask help"));
    }
    let name = first.as_deref().and_then(|value| value.to_str());
    if let Some(operation) = name.and_then(crate::release::rebuild::Operation::parse) {
        return crate::release::rebuild::run(root, operation);
    }
    match name {
        None | Some("help" | "--help" | "-h") => write_output(output, help::HELP),
        Some("product-package") => {
            let package = crate::product_package::build(root)?;
            write_output(output, &format!("product package ready: {}\n", package.display()))
        }
        Some("product-install") => {
            let package = crate::product_package::install(root)?;
            write_output(output, &format!("product installed: {}\n", package.display()))
        }
        Some("release-create") => crate::release::create(root),
        Some("release-package-stage") => crate::release::package_stage(root),
        Some("release-package-assemble") => crate::release::package_assemble(root),
        Some("release-stage") => {
            crate::release::stage_draft(root)?;
            write_output(output, "Release draft staged; publication remains a separate action.\n")
        }
        Some(command) if command.starts_with("distro-") => {
            let operation = crate::distro::Operation::parse(command).ok_or_else(|| {
                XtaskError::invocation("unsupported distribution operation; run cargo xtask help")
            })?;
            crate::distro::run(root, operation)
        }
        _ => Err(XtaskError::invocation("unsupported command; run cargo xtask help")),
    }
}

fn write_output(output: &mut dyn Write, text: &str) -> Result<(), XtaskError> {
    output
        .write_all(text.as_bytes())
        .map_err(|error| XtaskError::io("write command output to", Path::new("<stdout>"), error))
}

//! Reviewed, failure-propagating entry points for native rebuild observations.

use std::{env, path::Path, process::Command};

use crate::XtaskError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    Record,
    Library,
    CliLibrary,
    DaemonBinary,
    CliBinary,
    WindowsBinary,
}

impl Operation {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        match name {
            "release-rebuild-record" => Some(Self::Record),
            "release-daemon-library" => Some(Self::Library),
            "release-cli-library" => Some(Self::CliLibrary),
            "release-daemon-binary" => Some(Self::DaemonBinary),
            "release-cli-binary" => Some(Self::CliBinary),
            "release-windows-binary" => Some(Self::WindowsBinary),
            _ => None,
        }
    }
}

pub(crate) fn run(root: &Path, operation: Operation) -> Result<(), XtaskError> {
    let role = if matches!(operation, Operation::WindowsBinary) {
        None
    } else {
        Some(env::var("PERITUS_RELEASE_BUILD_ROLE").map_err(|_| {
            XtaskError::invocation("PERITUS_RELEASE_BUILD_ROLE must be primary or independent")
        })?)
    };
    let binary = if matches!(operation, Operation::WindowsBinary) {
        Some(env::var("PERITUS_RELEASE_BINARY").map_err(|_| {
            XtaskError::invocation("PERITUS_RELEASE_BINARY must select a reviewed native binary")
        })?)
    } else {
        None
    };
    super::run(
        &mut command(root, operation, role.as_deref(), binary.as_deref())?,
        "run native release build operation",
    )
}

fn command(
    root: &Path,
    operation: Operation,
    role: Option<&str>,
    binary: Option<&str>,
) -> Result<Command, XtaskError> {
    let mut command = Command::new(if cfg!(windows) { "python" } else { "python3" });
    let script = if matches!(
        operation,
        Operation::Library | Operation::CliLibrary | Operation::DaemonBinary | Operation::CliBinary
    ) {
        "packaging/native_build.py"
    } else if matches!(operation, Operation::WindowsBinary) {
        "packaging/windows_release.py"
    } else {
        "packaging/rebuild.py"
    };
    command.current_dir(root).arg(root.join(script));
    match operation {
        Operation::Record => {
            command.args(["record", "dist", require_role(role)?]);
        }
        Operation::Library => {
            command.env("PERITUS_RELEASE_BUILD_ROLE", require_role(role)?);
            command.arg("library");
        }
        Operation::CliLibrary => {
            command.env("PERITUS_RELEASE_BUILD_ROLE", require_role(role)?);
            command.args(["library", "peritus"]);
        }
        Operation::DaemonBinary | Operation::CliBinary => {
            let binary = if operation == Operation::DaemonBinary { "peritusd" } else { "peritus" };
            command.env("PERITUS_RELEASE_BUILD_ROLE", require_role(role)?);
            command.args(["binary", binary]);
        }
        Operation::WindowsBinary => {
            let binary = binary
                .filter(|value| {
                    matches!(
                        *value,
                        "peritus" | "peritusd" | "peritus-tui" | "peritus-windows-sandbox-helper"
                    )
                })
                .ok_or_else(|| {
                    XtaskError::invocation("release requires a reviewed Windows binary")
                })?;
            command.args(["build", binary]);
        }
    }
    Ok(command)
}

fn require_role(role: Option<&str>) -> Result<&str, XtaskError> {
    role.filter(|role| matches!(*role, "primary" | "independent"))
        .ok_or_else(|| XtaskError::invocation("release build role must be primary or independent"))
}

//! Reviewed, failure-propagating entry points for native rebuild observations.

use std::{env, path::Path, process::Command};

use crate::XtaskError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    Record,
    Compare,
    Library,
    DaemonBinary,
    CliBinary,
    WindowsBinary,
    WindowsSqliteCheck,
}

impl Operation {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        match name {
            "release-rebuild-record" => Some(Self::Record),
            "release-rebuild-compare" => Some(Self::Compare),
            "release-daemon-library" => Some(Self::Library),
            "release-daemon-binary" => Some(Self::DaemonBinary),
            "release-cli-binary" => Some(Self::CliBinary),
            "release-windows-binary" => Some(Self::WindowsBinary),
            "release-windows-sqlite-check" => Some(Self::WindowsSqliteCheck),
            _ => None,
        }
    }
}

pub(crate) fn run(root: &Path, operation: Operation) -> Result<(), XtaskError> {
    let role = if matches!(
        operation,
        Operation::Compare | Operation::WindowsBinary | Operation::WindowsSqliteCheck
    ) {
        None
    } else {
        Some(env::var("PERITUS_RELEASE_BUILD_ROLE").map_err(|_| {
            XtaskError::invocation("PERITUS_RELEASE_BUILD_ROLE must be primary or independent")
        })?)
    };
    let binary = if operation == Operation::WindowsBinary {
        Some(env::var("PERITUS_RELEASE_BINARY").map_err(|_| {
            XtaskError::invocation("PERITUS_RELEASE_BINARY must select a reviewed Windows binary")
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
    let script =
        if matches!(operation, Operation::Library | Operation::DaemonBinary | Operation::CliBinary)
        {
            "packaging/native_build.py"
        } else if matches!(operation, Operation::WindowsBinary | Operation::WindowsSqliteCheck) {
            "packaging/windows_release.py"
        } else {
            "packaging/rebuild.py"
        };
    command.current_dir(root).arg(root.join(script));
    match operation {
        Operation::Record => {
            command.args(["record", "dist", require_role(role)?]);
        }
        Operation::Compare => {
            command.args([
                "compare",
                "target/native-rebuild/primary",
                "target/native-rebuild/independent",
                "target/native-rebuild/report.json",
            ]);
        }
        Operation::Library => {
            command.env("PERITUS_RELEASE_BUILD_ROLE", require_role(role)?);
            command.arg("library");
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
        Operation::WindowsSqliteCheck => {
            command.arg("check-sqlite");
        }
    }
    Ok(command)
}

fn require_role(role: Option<&str>) -> Result<&str, XtaskError> {
    role.filter(|role| matches!(*role, "primary" | "independent"))
        .ok_or_else(|| XtaskError::invocation("release build role must be primary or independent"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_are_closed_and_commands_do_not_invoke_a_shell_or_product_build() {
        let root = Path::new(".");
        for role in [None, Some(""), Some("other"), Some("primary; exit 0")] {
            for operation in [
                Operation::Record,
                Operation::Library,
                Operation::DaemonBinary,
                Operation::CliBinary,
            ] {
                assert!(command(root, operation, role, None).is_err());
            }
        }
        for role in ["primary", "independent"] {
            let command = command(root, Operation::Record, Some(role), None).expect("record");
            assert_eq!(command.get_args().skip(1).collect::<Vec<_>>(), ["record", "dist", role]);
        }
        let command = command(root, Operation::Compare, None, None).expect("comparison");
        assert_eq!(
            command.get_args().skip(1).collect::<Vec<_>>(),
            [
                "compare",
                "target/native-rebuild/primary",
                "target/native-rebuild/independent",
                "target/native-rebuild/report.json",
            ]
        );
    }

    #[test]
    fn native_phases_select_only_the_reviewed_compiler_and_binary() {
        for (operation, arguments) in [
            (Operation::Library, vec!["library"]),
            (Operation::DaemonBinary, vec!["binary", "peritusd"]),
            (Operation::CliBinary, vec!["binary", "peritus"]),
        ] {
            for role in ["primary", "independent"] {
                let command = command(Path::new("."), operation, Some(role), None).expect("phase");
                let script = Path::new(".").join("packaging/native_build.py");
                assert_eq!(command.get_args().next(), Some(script.as_os_str()));
                assert_eq!(command.get_args().skip(1).collect::<Vec<_>>(), arguments);
                assert!(command.get_envs().any(|(name, value)| {
                    name == "PERITUS_RELEASE_BUILD_ROLE" && value == Some(role.as_ref())
                }));
            }
        }
    }

    #[test]
    fn windows_operations_reject_arbitrary_binaries_and_use_the_reviewed_script() {
        let root = Path::new(".");
        for binary in [None, Some(""), Some("other"), Some("peritus; exit 0")] {
            assert!(command(root, Operation::WindowsBinary, None, binary).is_err());
        }
        for binary in ["peritus", "peritusd", "peritus-tui", "peritus-windows-sandbox-helper"] {
            let command =
                command(root, Operation::WindowsBinary, None, Some(binary)).expect("binary");
            assert_eq!(command.get_args().skip(1).collect::<Vec<_>>(), ["build", binary]);
            assert_eq!(
                command.get_args().next(),
                Some(root.join("packaging/windows_release.py").as_os_str())
            );
        }
        let command =
            command(root, Operation::WindowsSqliteCheck, None, None).expect("SQLite check");
        assert_eq!(command.get_args().skip(1).collect::<Vec<_>>(), ["check-sqlite"]);
    }
}

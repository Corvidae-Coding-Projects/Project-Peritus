//! Reviewed, failure-propagating entry points for native rebuild observations.

use std::{env, path::Path, process::Command};

use crate::XtaskError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    Record,
    Compare,
    Library,
    Binary,
}

impl Operation {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        match name {
            "release-rebuild-record" => Some(Self::Record),
            "release-rebuild-compare" => Some(Self::Compare),
            "release-daemon-library" => Some(Self::Library),
            "release-daemon-binary" => Some(Self::Binary),
            _ => None,
        }
    }
}

pub(crate) fn run(root: &Path, operation: Operation) -> Result<(), XtaskError> {
    let role = if operation == Operation::Compare {
        None
    } else {
        Some(env::var("PERITUS_RELEASE_BUILD_ROLE").map_err(|_| {
            XtaskError::invocation("PERITUS_RELEASE_BUILD_ROLE must be primary or independent")
        })?)
    };
    super::run(
        &mut command(root, operation, role.as_deref())?,
        "run native release build operation",
    )
}

fn command(root: &Path, operation: Operation, role: Option<&str>) -> Result<Command, XtaskError> {
    let mut command = Command::new(if cfg!(windows) { "python" } else { "python3" });
    let script = if matches!(operation, Operation::Library | Operation::Binary) {
        "packaging/native_build.py"
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
        Operation::Library | Operation::Binary => {
            command.env("PERITUS_RELEASE_BUILD_ROLE", require_role(role)?);
            command.arg(if operation == Operation::Library { "library" } else { "binary" });
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
            for operation in [Operation::Record, Operation::Library, Operation::Binary] {
                assert!(command(root, operation, role).is_err());
            }
        }
        for role in ["primary", "independent"] {
            let command = command(root, Operation::Record, Some(role)).expect("record");
            assert_eq!(command.get_args().skip(1).collect::<Vec<_>>(), ["record", "dist", role]);
        }
        let command = command(root, Operation::Compare, None).expect("comparison");
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
    fn native_daemon_phases_select_only_the_reviewed_compiler_entry_point() {
        for (operation, phase) in [(Operation::Library, "library"), (Operation::Binary, "binary")] {
            for role in ["primary", "independent"] {
                let command = command(Path::new("."), operation, Some(role)).expect("phase");
                let script = Path::new(".").join("packaging/native_build.py");
                assert_eq!(command.get_args().next(), Some(script.as_os_str()));
                assert_eq!(command.get_args().skip(1).collect::<Vec<_>>(), [phase]);
                assert!(command.get_envs().any(|(name, value)| {
                    name == "PERITUS_RELEASE_BUILD_ROLE" && value == Some(role.as_ref())
                }));
            }
        }
    }
}

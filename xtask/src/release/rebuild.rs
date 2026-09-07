//! Reviewed, failure-propagating entry points for native rebuild observations.

use std::{env, path::Path, process::Command};

use crate::XtaskError;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operation {
    Record,
    Compare,
}

pub(crate) fn run(root: &Path, operation: Operation) -> Result<(), XtaskError> {
    let role = if operation == Operation::Record {
        Some(env::var("PERITUS_RELEASE_BUILD_ROLE").map_err(|_| {
            XtaskError::invocation("PERITUS_RELEASE_BUILD_ROLE must be primary or independent")
        })?)
    } else {
        None
    };
    super::run(&mut command(root, operation, role.as_deref())?, "verify native rebuild evidence")
}

fn command(root: &Path, operation: Operation, role: Option<&str>) -> Result<Command, XtaskError> {
    let mut command = Command::new(if cfg!(windows) { "python" } else { "python3" });
    command.current_dir(root).arg(root.join("packaging/rebuild.py"));
    match operation {
        Operation::Record => {
            let role = role.filter(|role| matches!(*role, "primary" | "independent")).ok_or_else(
                || XtaskError::invocation("release build role must be primary or independent"),
            )?;
            command.args(["record", "dist", role]);
        }
        Operation::Compare => {
            command.args([
                "compare",
                "target/native-rebuild/primary",
                "target/native-rebuild/independent",
                "target/native-rebuild/report.json",
            ]);
        }
    }
    Ok(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_are_closed_and_commands_do_not_invoke_a_shell_or_product_build() {
        let root = Path::new(".");
        for role in [None, Some(""), Some("other"), Some("primary; exit 0")] {
            assert!(command(root, Operation::Record, role).is_err());
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
}

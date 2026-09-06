//! Read-only inspection of a role-scoped published local context.

use peritus_role::HarnessRole;
use peritus_types::{RunId, WorkspaceId};
use std::{ffi::OsString, path::PathBuf, process::ExitCode};

pub(super) struct Inspection {
    trace: PathBuf,
    run: RunId,
    workspace: WorkspaceId,
    role: HarnessRole,
}

pub(super) fn parse(arguments: &mut impl Iterator<Item = OsString>) -> Option<Inspection> {
    let mut trace = None;
    let mut run = None;
    let mut workspace = None;
    let mut role = None;
    while let Some(flag) = arguments.next() {
        let value = arguments.next()?;
        match flag.to_str()? {
            "--trace" if trace.is_none() && !value.is_empty() => trace = Some(PathBuf::from(value)),
            "--run" if run.is_none() => run = Some(RunId::new(identity(value.to_str()?)?).ok()?),
            "--workspace" if workspace.is_none() => {
                workspace = Some(WorkspaceId::new(identity(value.to_str()?)?).ok()?);
            }
            "--role" if role.is_none() => {
                role = Some(match value.to_str()? {
                    "writer" => HarnessRole::Writer,
                    "fixer" => HarnessRole::Fixer,
                    "reviewer" => HarnessRole::Reviewer,
                    _ => return None,
                });
            }
            _ => return None,
        }
    }
    Some(Inspection { trace: trace?, run: run?, workspace: workspace?, role: role? })
}

fn identity(value: &str) -> Option<[u8; 16]> {
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let mut bytes = [0; 16];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

pub(super) fn run(arguments: Inspection) -> ExitCode {
    match peritus_product_runner::inspect_local_context(
        &arguments.trace,
        arguments.run,
        arguments.workspace,
        arguments.role,
    ) {
        Ok(output) => {
            super::write_output(&output).map_or_else(super::output_failure, |()| ExitCode::SUCCESS)
        }
        Err(error) => {
            super::write_error(&error.to_string());
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn arguments() -> Vec<OsString> {
        [
            "--trace",
            "run.trace",
            "--run",
            "01010101010101010101010101010101",
            "--workspace",
            "02020202020202020202020202020202",
            "--role",
            "writer",
        ]
        .map(OsString::from)
        .to_vec()
    }

    #[test]
    fn inspection_requires_exact_identity_and_rejects_duplicates_or_unknown_flags() {
        assert!(parse(&mut arguments().into_iter()).is_some());
        for extra in [["--run", "01010101010101010101010101010101"], ["--repair", "true"]] {
            let mut values = arguments();
            values.extend(extra.map(OsString::from));
            assert!(parse(&mut values.into_iter()).is_none());
        }
        for invalid in ["x", "00000000000000000000000000000000", "éééééééééééééééé"]
        {
            let mut values = arguments();
            values[3] = invalid.into();
            assert!(parse(&mut values.into_iter()).is_none());
        }
        let mut values = arguments();
        values.pop();
        assert!(parse(&mut values.into_iter()).is_none());
    }
}

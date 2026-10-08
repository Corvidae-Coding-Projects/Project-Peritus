//! Read-only inspection of a role-scoped published local context.

use peritus_role::HarnessRole;
use peritus_types::{RunId, WorkspaceId};
use std::{ffi::OsString, io::Write, path::PathBuf, process::ExitCode};

#[derive(Clone, Copy)]
enum Section {
    Complete,
    Summary,
    Sources,
    Messages,
    Lineage,
    View,
    Archive,
}

pub(super) struct Inspection {
    trace: PathBuf,
    run: RunId,
    workspace: WorkspaceId,
    role: HarnessRole,
    section: Section,
    offset: u64,
    maximum: Option<usize>,
}

pub(super) fn parse(arguments: &mut impl Iterator<Item = OsString>) -> Option<Inspection> {
    let mut trace = None;
    let mut run = None;
    let mut workspace = None;
    let mut role = None;
    let mut section = None;
    let mut offset = None;
    let mut maximum = None;
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
            "--section" if section.is_none() => {
                section = Some(match value.to_str()? {
                    "complete" => Section::Complete,
                    "summary" => Section::Summary,
                    "sources" => Section::Sources,
                    "messages" => Section::Messages,
                    "lineage" => Section::Lineage,
                    "view" => Section::View,
                    "archive" => Section::Archive,
                    _ => return None,
                });
            }
            "--offset" if offset.is_none() => offset = Some(value.to_str()?.parse().ok()?),
            "--limit" if maximum.is_none() => {
                let count = value.to_str()?.parse::<usize>().ok()?;
                if count == 0 { return None; }
                maximum = Some(count);
            }
            _ => return None,
        }
    }
    let section = section.unwrap_or(Section::Complete);
    if matches!(section, Section::Complete | Section::Summary)
        && (offset.is_some() || maximum.is_some())
    {
        return None;
    }
    Some(Inspection {
        trace: trace?, run: run?, workspace: workspace?, role: role?,
        section, offset: offset.unwrap_or(0), maximum,
    })
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
    match write_inspection(arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            super::write_error(&error);
            ExitCode::FAILURE
        }
    }
}

fn write_inspection(arguments: Inspection) -> Result<(), String> {
    if matches!(arguments.section, Section::Complete) {
        let output = peritus_product_runner::inspect_local_context(
            &arguments.trace,
            arguments.run,
            arguments.workspace,
            arguments.role,
        ).map_err(|error| error.to_string())?;
        return super::write_output(&output).map_err(|error| error.to_string());
    }
    let mut inspection = peritus_product_runner::open_local_context_inspection(
        &arguments.trace,
        arguments.run,
        arguments.workspace,
        arguments.role,
    ).map_err(|error| error.to_string())?;
    let output = match arguments.section {
        Section::Summary => inspection.summary(),
        Section::Sources => inspection.source_page(
            usize::try_from(arguments.offset).map_err(|error| error.to_string())?,
            arguments.maximum.unwrap_or(256),
        ),
        Section::Messages => inspection.message_page(
            usize::try_from(arguments.offset).map_err(|error| error.to_string())?,
            arguments.maximum.unwrap_or(16),
        ),
        Section::Lineage => inspection.lineage_page(
            usize::try_from(arguments.offset).map_err(|error| error.to_string())?,
            arguments.maximum.unwrap_or(256),
        ),
        Section::View | Section::Archive => {
            let mut offset = arguments.offset;
            let mut stdout = std::io::stdout().lock();
            while let Some(chunk) = inspection.read_view(offset, arguments.maximum.unwrap_or(64 * 1024))
                .map_err(|error| error.to_string())?
            {
                stdout.write_all(chunk.bytes()).map_err(|error| error.to_string())?;
                offset = offset.checked_add(u64::try_from(chunk.bytes().len()).map_err(|error| error.to_string())?)
                    .ok_or_else(|| "inspection archive offset overflow".to_owned())?;
                if matches!(arguments.section, Section::View) { break; }
            }
            return stdout.flush().map_err(|error| error.to_string());
        }
        Section::Complete => return Err("complete inspection was already handled".to_owned()),
    }.map_err(|error| error.to_string())?;
    super::write_output(&output).map_err(|error| error.to_string())
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

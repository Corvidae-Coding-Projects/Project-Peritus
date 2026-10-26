//! Deterministic source-file readability verification for product candidates.

use std::{fmt::Write as _, fs, io, path::Path};

use peritus_gates::{GateExecutionRecord, ProjectKind};

use super::cancellation::GateCancellation;
use crate::developer_tools::WorkspaceOwnership;

pub fn run(
    workspace_root: &Path,
    project_root: &Path,
    changed_paths: &[std::path::PathBuf],
    kind: ProjectKind,
    command: String,
    ownership: Option<&WorkspaceOwnership>,
    cancellation: &GateCancellation,
) -> GateExecutionRecord {
    let mut source_files = changed_paths
        .iter()
        .filter(|path| path.starts_with(project_root) && is_source(path, kind))
        .cloned()
        .collect::<Vec<_>>();
    source_files.sort();
    source_files.dedup();
    let mut files = source_files
        .iter()
        .filter(|path| {
            ownership.is_none_or(|scope| scope.source_layout_applies(&workspace_root.join(path)))
        })
        .cloned()
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    let mut errors = Vec::new();
    let mut unevaluated = false;

    for path in &files {
        if cancellation.is_cancelled() {
            unevaluated = true;
            break;
        }
        let full_path = workspace_root.join(path);
        match fs::symlink_metadata(&full_path) {
            Ok(metadata) if metadata.file_type().is_file() => {
                if let Err(error) = read_source(&full_path, cancellation) {
                    if cancellation.is_cancelled() {
                        unevaluated = true;
                        break;
                    }
                    errors.push(format!("{}: read source: {error}", path.display()));
                }
            }
            Ok(_) => {
                errors.push(format!("{}: selected source is not a regular file", path.display()));
            }
            Err(error) => errors.push(format!("{}: inspect source: {error}", path.display())),
        }
    }

    let passed = errors.is_empty();
    let mut output = format!("Read {} changed source file(s).\n", files.len());
    let skipped = source_files.len().saturating_sub(files.len());
    if skipped > 0 {
        let _ = write!(
            output,
            "Skipped {skipped} imported or command-generated source file(s) that were not part of the starting workspace and were not created through the explicit write tool."
        );
        output.push('\n');
    }
    for error in &errors {
        output.push_str("ERROR: ");
        output.push_str(error);
        output.push('\n');
    }
    output.push_str(if !passed {
        "Source readability: FAIL\n"
    } else if unevaluated {
        "Source readability: NOT EVALUATED (run was cancelled)\n"
    } else {
        "Source readability: PASS\n"
    });

    GateExecutionRecord {
        command,
        label: "Source readability".to_owned(),
        exit_code: if !passed {
            Some(1)
        } else if unevaluated {
            None
        } else {
            Some(0)
        },
        output,
    }
}

fn read_source(path: &Path, cancellation: &GateCancellation) -> Result<u64, io::Error> {
    let file = fs::File::open(path)?;
    io::copy(&mut cancellation.reader(file), &mut io::sink())
}

fn is_source(path: &Path, kind: ProjectKind) -> bool {
    let extension = path.extension().and_then(std::ffi::OsStr::to_str);
    match kind {
        ProjectKind::Generic | ProjectKind::Make => true,
        ProjectKind::Artifact => extension.is_some_and(|value| {
            ["c", "cc", "cpp", "go", "h", "hpp", "js", "jsx", "mjs", "py", "rs", "sh", "ts", "tsx"]
                .contains(&value)
        }),
        ProjectKind::Rust => extension == Some("rs"),
        ProjectKind::Node => {
            extension.is_some_and(|value| ["js", "jsx", "mjs", "cjs", "ts", "tsx"].contains(&value))
        }
        ProjectKind::Python => extension == Some("py"),
        ProjectKind::Sqlite => extension == Some("sql"),
        ProjectKind::Go => extension == Some("go"),
    }
}

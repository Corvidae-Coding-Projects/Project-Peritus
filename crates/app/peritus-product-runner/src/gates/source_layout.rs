//! Deterministic source-file readability verification for product candidates.

use std::{fmt::Write as _, fs, path::Path};

use peritus_gates::{GateExecutionRecord, ProjectKind};

use crate::developer_tools::WorkspaceOwnership;

pub fn run(
    workspace_root: &Path,
    project_root: &Path,
    changed_paths: &[std::path::PathBuf],
    kind: ProjectKind,
    command: String,
    ownership: Option<&WorkspaceOwnership>,
) -> GateExecutionRecord {
    let mut source_files = changed_paths
        .iter()
        .filter(|path| path.starts_with(project_root) && is_source(path, kind))
        .filter(|path| {
            fs::symlink_metadata(workspace_root.join(path))
                .is_ok_and(|metadata| metadata.file_type().is_file())
        })
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

    for path in &files {
        if let Err(error) = fs::read_to_string(workspace_root.join(path)) {
            errors.push(format!("{}: read source: {error}", path.display()));
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
    output.push_str(if passed {
        "Source readability: PASS\n"
    } else {
        "Source readability: FAIL\n"
    });

    GateExecutionRecord {
        command,
        label: "Source readability".to_owned(),
        exit_code: Some(i32::from(!passed)),
        output,
    }
}

fn is_source(path: &Path, kind: ProjectKind) -> bool {
    let extension = path.extension().and_then(std::ffi::OsStr::to_str);
    match kind {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_a_large_readable_rust_source_file() {
        let root = tempfile::tempdir().expect("source root");
        let source = "pub const VALUE: usize = 1;\n".repeat(700);
        fs::write(root.path().join("large.rs"), source).expect("large source");

        let record = run(
            root.path(),
            Path::new(""),
            &[std::path::PathBuf::from("large.rs")],
            ProjectKind::Rust,
            "source-readability".to_owned(),
            None,
        );

        assert_eq!(record.exit_code, Some(0));
        assert!(record.output.contains("Read 1 changed source file"));
    }

    #[test]
    fn ignores_unchanged_and_generated_sources() {
        let root = tempfile::tempdir().expect("source root");
        fs::create_dir_all(root.path().join("src")).expect("source directory");
        fs::create_dir_all(root.path().join("target/generated")).expect("target directory");
        fs::write(root.path().join("src/lib.rs"), "pub const VALUE: usize = 1;\n")
            .expect("source file");
        fs::write(root.path().join("src/legacy.rs"), "line\n".repeat(700)).expect("legacy source");
        fs::write(root.path().join("target/generated/large.rs"), "line\n".repeat(700))
            .expect("generated output");

        let record = run(
            root.path(),
            Path::new(""),
            &[std::path::PathBuf::from("src/lib.rs"), std::path::PathBuf::from("target")],
            ProjectKind::Rust,
            "source-readability".to_owned(),
            None,
        );

        assert_eq!(record.exit_code, Some(0));
        assert!(record.output.contains("Read 1 changed source file"));
        assert!(!record.output.contains("legacy.rs"));
        assert!(!record.output.contains("large.rs"));
    }

    #[test]
    fn product_scope_keeps_first_party_files_strict_and_skips_late_imports() {
        let root = tempfile::tempdir().expect("source root");
        let baseline = root.path().join("legacy.c");
        fs::write(&baseline, "line\n".repeat(700)).expect("baseline source");
        let mut ownership = WorkspaceOwnership::capture(root.path());

        let imported = root.path().join("upstream.c");
        fs::write(&imported, "line\n".repeat(700)).expect("imported source");
        let direct = root.path().join("authored.c");
        fs::write(&direct, "line\n".repeat(700)).expect("direct source");
        ownership.record_direct_creation(&direct, false);
        let changed = [
            std::path::PathBuf::from("legacy.c"),
            std::path::PathBuf::from("upstream.c"),
            std::path::PathBuf::from("authored.c"),
        ];

        let record = run(
            root.path(),
            Path::new(""),
            &changed,
            ProjectKind::Artifact,
            "source-readability".to_owned(),
            Some(&ownership),
        );

        assert_eq!(record.exit_code, Some(0));
        assert!(record.output.contains("Skipped 1 imported or command-generated"));
    }
}

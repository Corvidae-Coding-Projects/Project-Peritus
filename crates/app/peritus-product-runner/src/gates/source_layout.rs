//! Deterministic source-file size enforcement for product candidates.

use std::{fs, path::Path};

use peritus_gates::{GateExecutionRecord, PRODUCT_MAX_SOURCE_LINES, ProjectKind};

pub fn run(
    workspace_root: &Path,
    project_root: &Path,
    changed_paths: &[std::path::PathBuf],
    kind: ProjectKind,
    command: String,
) -> GateExecutionRecord {
    let mut files = changed_paths
        .iter()
        .filter(|path| path.starts_with(project_root) && is_source(path, kind))
        .filter(|path| {
            fs::symlink_metadata(workspace_root.join(path))
                .is_ok_and(|metadata| metadata.file_type().is_file())
        })
        .cloned()
        .collect::<Vec<_>>();
    files.sort();
    files.dedup();
    let mut errors = Vec::new();

    let mut violations = Vec::new();
    for path in &files {
        match fs::read_to_string(workspace_root.join(path)) {
            Ok(source) => {
                let lines = source.lines().count();
                if lines > PRODUCT_MAX_SOURCE_LINES {
                    violations.push(format!(
                        "{}: {lines} lines exceeds the {PRODUCT_MAX_SOURCE_LINES}-line hard limit",
                        path.display(),
                    ));
                }
            }
            Err(error) => {
                errors.push(format!("{}: read source: {error}", path.display()));
            }
        }
    }

    let passed = violations.is_empty() && errors.is_empty();
    let mut output = format!(
        "Scanned {} changed source file(s); hard limit: {PRODUCT_MAX_SOURCE_LINES} lines.\n",
        files.len(),
    );
    for violation in &violations {
        output.push_str("VIOLATION: ");
        output.push_str(violation);
        output.push('\n');
    }
    for error in &errors {
        output.push_str("ERROR: ");
        output.push_str(error);
        output.push('\n');
    }
    output.push_str(if passed { "Source layout: PASS\n" } else { "Source layout: FAIL\n" });

    GateExecutionRecord {
        command,
        label: "Source layout".to_owned(),
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
    use std::fmt::Write as _;

    use super::*;

    #[test]
    fn rejects_a_rust_source_file_over_the_hard_limit() {
        let root = tempfile::tempdir().expect("source root");
        let source = (0..=PRODUCT_MAX_SOURCE_LINES).fold(String::new(), |mut source, index| {
            let _ = write!(source, "pub const VALUE_{index}: usize = {index};");
            source.push('\n');
            source
        });
        fs::write(root.path().join("large.rs"), source).expect("large source");

        let record = run(
            root.path(),
            Path::new(""),
            &[std::path::PathBuf::from("large.rs")],
            ProjectKind::Rust,
            "source-layout".to_owned(),
        );

        assert_eq!(record.exit_code, Some(1));
        assert!(record.output.contains("large.rs: 501 lines"));
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
            "source-layout".to_owned(),
        );

        assert_eq!(record.exit_code, Some(0));
        assert!(record.output.contains("Scanned 1 changed source file"));
        assert!(!record.output.contains("legacy.rs"));
        assert!(!record.output.contains("large.rs"));
    }
}

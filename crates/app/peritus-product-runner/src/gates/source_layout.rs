//! Deterministic source-file size enforcement for product candidates.

use std::{
    fs,
    path::{Path, PathBuf},
};

use peritus_gates::{GateExecutionRecord, PRODUCT_MAX_SOURCE_LINES, ProjectKind};

const IGNORED_DIRECTORIES: &[&str] =
    &[".git", ".peritus", "build", "dist", "node_modules", "target", "vendor"];

pub fn run(root: &Path, kind: ProjectKind, command: String) -> GateExecutionRecord {
    let mut files = Vec::new();
    let mut errors = Vec::new();
    discover(root, root, kind, &mut files, &mut errors);
    files.sort();

    let mut violations = Vec::new();
    for path in &files {
        match fs::read_to_string(path) {
            Ok(source) => {
                let lines = source.lines().count();
                if lines > PRODUCT_MAX_SOURCE_LINES {
                    violations.push(format!(
                        "{}: {lines} lines exceeds the {PRODUCT_MAX_SOURCE_LINES}-line hard limit",
                        relative(root, path).display(),
                    ));
                }
            }
            Err(error) => {
                errors.push(format!("{}: read source: {error}", relative(root, path).display()));
            }
        }
    }

    let passed = violations.is_empty() && errors.is_empty();
    let mut output = format!(
        "Scanned {} source file(s); hard limit: {PRODUCT_MAX_SOURCE_LINES} lines.\n",
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

fn discover(
    root: &Path,
    directory: &Path,
    kind: ProjectKind,
    files: &mut Vec<PathBuf>,
    errors: &mut Vec<String>,
) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            errors
                .push(format!("{}: read directory: {error}", relative(root, directory).display()));
            return;
        }
    };
    let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let path = entry.path();
        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                errors.push(format!("{}: inspect path: {error}", relative(root, &path).display()));
                continue;
            }
        };
        if file_type.is_symlink() {
            continue;
        }
        if file_type.is_dir() {
            if !ignored_directory(&entry.file_name()) {
                discover(root, &path, kind, files, errors);
            }
        } else if file_type.is_file() && is_source(&path, kind) {
            files.push(path);
        }
    }
}

fn ignored_directory(name: &std::ffi::OsStr) -> bool {
    name.to_str().is_some_and(|name| IGNORED_DIRECTORIES.contains(&name))
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
        ProjectKind::Go => extension == Some("go"),
    }
}

fn relative<'a>(root: &'a Path, path: &'a Path) -> &'a Path {
    path.strip_prefix(root).unwrap_or(path)
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

        let record = run(root.path(), ProjectKind::Rust, "source-layout".to_owned());

        assert_eq!(record.exit_code, Some(1));
        assert!(record.output.contains("large.rs: 501 lines"));
    }

    #[test]
    fn ignores_build_outputs_and_accepts_modular_source() {
        let root = tempfile::tempdir().expect("source root");
        fs::create_dir_all(root.path().join("src")).expect("source directory");
        fs::create_dir_all(root.path().join("target/generated")).expect("target directory");
        fs::write(root.path().join("src/lib.rs"), "pub const VALUE: usize = 1;\n")
            .expect("source file");
        fs::write(root.path().join("target/generated/large.rs"), "line\n".repeat(700))
            .expect("generated output");

        let record = run(root.path(), ProjectKind::Rust, "source-layout".to_owned());

        assert_eq!(record.exit_code, Some(0));
        assert!(record.output.contains("Scanned 1 source file"));
    }
}

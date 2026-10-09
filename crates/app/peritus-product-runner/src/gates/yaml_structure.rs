//! Deterministic syntax validation for changed YAML configuration files.

use std::{fs, path::Path};

use peritus_gates::GateExecutionRecord;
use yaml_rust2::YamlLoader;

const MAX_YAML_BYTES: u64 = 16 * 1024 * 1024;

#[allow(
    clippy::format_push_string,
    reason = "formal-boundary policy models format! but not writeln!"
)]
pub fn run(
    workspace_root: &Path,
    project_root: &Path,
    changed_paths: &[std::path::PathBuf],
    command: String,
) -> GateExecutionRecord {
    let yaml_paths = changed_paths
        .iter()
        .filter(|path| path.starts_with(project_root) && is_yaml(path))
        .collect::<Vec<_>>();
    let mut output = String::new();
    let mut passed = true;

    for relative in &yaml_paths {
        match validate_file(&workspace_root.join(relative)) {
            Ok(document_count) => {
                output.push_str(&format!(
                    "{}: PASS ({document_count} document(s))\n",
                    relative.display(),
                ));
            }
            Err(detail) => {
                passed = false;
                output.push_str(&format!("{}: FAIL: {detail}\n", relative.display()));
            }
        }
    }

    if yaml_paths.is_empty() {
        output.push_str("No changed YAML files require structural validation.\n");
    }
    output.push_str(if passed { "YAML structure: PASS\n" } else { "YAML structure: FAIL\n" });

    GateExecutionRecord {
        command,
        label: "YAML structure".to_owned(),
        exit_code: Some(i32::from(!passed)),
        output,
    }
}

fn is_yaml(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| matches!(extension.to_ascii_lowercase().as_str(), "yml" | "yaml"))
}

fn validate_file(path: &Path) -> Result<usize, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("inspect file: {error}"))?;
    if metadata.len() > MAX_YAML_BYTES {
        return Err(format!("file exceeds the {MAX_YAML_BYTES}-byte validation limit"));
    }
    let source = fs::read_to_string(path).map_err(|error| format!("read file: {error}"))?;
    if source.trim().is_empty() {
        return Err("file is empty".to_owned());
    }
    let documents = YamlLoader::load_from_str(&source).map_err(|error| error.to_string())?;
    if documents.is_empty() {
        return Err("file has no YAML documents".to_owned());
    }
    Ok(documents.len())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn validates_only_changed_yaml_files_in_the_project() {
        let root = tempfile::tempdir().expect("workspace");
        fs::create_dir_all(root.path().join("project/.github/workflows")).expect("directory");
        fs::write(
            root.path().join("project/.github/workflows/ci.yml"),
            "name: CI\non: [push]\njobs: {}\n",
        )
        .expect("workflow");
        fs::write(root.path().join("outside.yaml"), "broken: [\n").expect("outside YAML");

        let record = run(
            root.path(),
            Path::new("project"),
            &[PathBuf::from("project/.github/workflows/ci.yml"), PathBuf::from("outside.yaml")],
            "yaml-structure".to_owned(),
        );

        assert_eq!(record.exit_code, Some(0));
        assert!(record.output.contains("project/.github/workflows/ci.yml: PASS"));
        assert!(!record.output.contains("outside.yaml"));
    }

    #[test]
    fn rejects_malformed_yaml() {
        let root = tempfile::tempdir().expect("workspace");
        fs::write(root.path().join("broken.yml"), "jobs: [\n").expect("malformed YAML");

        let record = run(
            root.path(),
            Path::new(""),
            &[PathBuf::from("broken.yml")],
            "yaml-structure".to_owned(),
        );

        assert_eq!(record.exit_code, Some(1));
        assert!(record.output.contains("broken.yml: FAIL"));
    }
}

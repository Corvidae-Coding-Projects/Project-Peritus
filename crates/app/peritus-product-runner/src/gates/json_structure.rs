//! Deterministic syntax validation for changed JSON files.

use std::{fs, path::Path};

use peritus_gates::GateExecutionRecord;

const MAX_JSON_BYTES: u64 = 16 * 1024 * 1024;

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
    let json_paths = changed_paths
        .iter()
        .filter(|path| path.starts_with(project_root) && is_json(path))
        .collect::<Vec<_>>();
    let mut output = String::new();
    let mut passed = true;

    for relative in &json_paths {
        match validate_file(&workspace_root.join(relative)) {
            Ok(kind) => {
                output.push_str(&format!("{}: PASS ({kind})\n", relative.display()));
            }
            Err(detail) => {
                passed = false;
                output.push_str(&format!("{}: FAIL: {detail}\n", relative.display()));
            }
        }
    }

    if json_paths.is_empty() {
        output.push_str("No changed JSON files require structural validation.\n");
    }
    output.push_str(if passed { "JSON structure: PASS\n" } else { "JSON structure: FAIL\n" });

    GateExecutionRecord {
        command,
        label: "JSON structure".to_owned(),
        exit_code: Some(i32::from(!passed)),
        output,
    }
}

fn is_json(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

fn validate_file(path: &Path) -> Result<&'static str, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("inspect file: {error}"))?;
    if metadata.len() > MAX_JSON_BYTES {
        return Err(format!("file exceeds the {MAX_JSON_BYTES}-byte validation limit"));
    }
    let bytes = fs::read(path).map_err(|error| format!("read file: {error}"))?;
    let value = serde_json::from_slice::<serde_json::Value>(&bytes)
        .map_err(|error| format!("parse JSON: {error}"))?;
    Ok(match value {
        serde_json::Value::Null => "null",
        serde_json::Value::Bool(_) => "boolean",
        serde_json::Value::Number(_) => "number",
        serde_json::Value::String(_) => "string",
        serde_json::Value::Array(_) => "array",
        serde_json::Value::Object(_) => "object",
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn validates_only_changed_json_files_in_the_project() {
        let root = tempfile::tempdir().expect("workspace");
        fs::create_dir_all(root.path().join("project/out")).expect("directory");
        fs::write(root.path().join("project/out/result.json"), br#"{"result":true}"#)
            .expect("result");
        fs::write(root.path().join("outside.json"), b"{").expect("outside JSON");

        let record = run(
            root.path(),
            Path::new("project"),
            &[PathBuf::from("project/out/result.json"), PathBuf::from("outside.json")],
            "json-structure".to_owned(),
        );

        assert_eq!(record.exit_code, Some(0));
        assert!(record.output.contains("project/out/result.json: PASS (object)"));
        assert!(!record.output.contains("outside.json"));
    }

    #[test]
    fn rejects_malformed_json() {
        let root = tempfile::tempdir().expect("workspace");
        fs::write(root.path().join("broken.json"), b"{\"value\":").expect("malformed JSON");

        let record = run(
            root.path(),
            Path::new(""),
            &[PathBuf::from("broken.json")],
            "json-structure".to_owned(),
        );

        assert_eq!(record.exit_code, Some(1));
        assert!(record.output.contains("broken.json: FAIL: parse JSON"));
    }
}

//! Deterministic syntax validation for changed JSON files.

use std::{fs, path::Path};

use peritus_gates::GateExecutionRecord;
use serde::Deserializer as _;
use serde::de::{IgnoredAny, MapAccess, SeqAccess, Visitor};

use super::cancellation::GateCancellation;

#[allow(
    clippy::format_push_string,
    reason = "formal-boundary policy models format! but not writeln!"
)]
pub fn run(
    workspace_root: &Path,
    project_root: &Path,
    changed_paths: &[std::path::PathBuf],
    command: String,
    cancellation: &GateCancellation,
) -> GateExecutionRecord {
    let json_paths = changed_paths
        .iter()
        .filter(|path| path.starts_with(project_root) && is_json(path))
        .collect::<Vec<_>>();
    let mut output = String::new();
    let mut passed = true;
    let mut unevaluated = false;

    for relative in &json_paths {
        if cancellation.is_cancelled() {
            unevaluated = true;
            output.push_str("JSON structure: NOT EVALUATED (run was cancelled)\n");
            break;
        }
        match validate_file(&workspace_root.join(relative), cancellation) {
            Ok(kind) => {
                output.push_str(&format!("{}: PASS ({kind})\n", relative.display()));
            }
            Err(detail) => {
                if cancellation.is_cancelled() {
                    unevaluated = true;
                    output.push_str(&format!(
                        "{}: NOT EVALUATED (run was cancelled while reading or parsing)\n",
                        relative.display(),
                    ));
                    break;
                }
                passed = false;
                output.push_str(&format!("{}: FAIL: {detail}\n", relative.display()));
            }
        }
    }

    if json_paths.is_empty() {
        output.push_str("No changed JSON files require structural validation.\n");
    }
    output.push_str(if !passed {
        "JSON structure: FAIL\n"
    } else if unevaluated {
        "JSON structure: NOT EVALUATED\n"
    } else {
        "JSON structure: PASS\n"
    });

    GateExecutionRecord {
        command,
        label: "JSON structure".to_owned(),
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

fn is_json(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

fn validate_file(path: &Path, cancellation: &GateCancellation) -> Result<&'static str, String> {
    let file = fs::File::open(path).map_err(|error| format!("read file: {error}"))?;
    let mut parser = serde_json::Deserializer::from_reader(cancellation.reader(file));
    let kind = parser.deserialize_any(JsonKind).map_err(|error| format!("parse JSON: {error}"))?;
    parser.end().map_err(|error| format!("parse JSON: {error}"))?;
    Ok(kind)
}

struct JsonKind;

impl<'de> Visitor<'de> for JsonKind {
    type Value = &'static str;

    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
        Ok("null")
    }

    fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
        Ok("boolean")
    }

    fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self::Value, E> {
        Ok("number")
    }

    fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self::Value, E> {
        Ok("number")
    }

    fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
        Ok("number")
    }

    fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Self::Value, E> {
        Ok("string")
    }

    fn visit_string<E: serde::de::Error>(self, _: String) -> Result<Self::Value, E> {
        Ok("string")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        while sequence.next_element::<IgnoredAny>()?.is_some() {}
        Ok("array")
    }

    fn visit_map<A>(self, mut mapping: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        while mapping.next_entry::<IgnoredAny, IgnoredAny>()?.is_some() {}
        Ok("object")
    }
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
            &GateCancellation::default(),
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
            &GateCancellation::default(),
        );

        assert_eq!(record.exit_code, Some(1));
        assert!(record.output.contains("broken.json: FAIL: parse JSON"));
    }
}

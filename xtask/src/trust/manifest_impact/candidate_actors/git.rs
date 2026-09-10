//! Exact regular-file loading from a detached Git tree.

use crate::error::Diagnostic;
use crate::trust::manifest_file;
use serde::de::DeserializeOwned;
use std::path::Path;
use std::process::{Command, Output};

pub(super) fn tree_object_exists(
    root: &Path,
    tree: &str,
    diagnostic_path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    if !full_object_id(tree) {
        candidate_error(
            diagnostic_path,
            format!("candidate implementation tree `{tree}` is not a full Git object ID"),
            diagnostics,
        );
        return false;
    }
    let Some(output) =
        command(root, &["cat-file", "-t", tree], Path::new(diagnostic_path), diagnostics)
    else {
        return false;
    };
    if !output.status.success() || output.stdout != b"tree\n" {
        candidate_error(
            diagnostic_path,
            format!("candidate implementation tree `{tree}` is not an available tree object"),
            diagnostics,
        );
        return false;
    }
    true
}

pub(super) fn load_tree_toml<T: DeserializeOwned>(
    root: &Path,
    tree: &str,
    path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(T, Vec<u8>)> {
    let bytes = load_regular_blob(root, tree, path, diagnostics)?;
    let text = std::str::from_utf8(&bytes).map_err(|error| error.to_string());
    match text.and_then(|text| toml::from_str(text).map_err(|error| error.to_string())) {
        Ok(document) => Some((document, bytes)),
        Err(error) => {
            parse_error(path, "TOML", &error, diagnostics);
            None
        }
    }
}

pub(super) fn load_tree_json<T: DeserializeOwned>(
    root: &Path,
    tree: &str,
    path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<(T, Vec<u8>)> {
    let bytes = load_regular_blob(root, tree, path, diagnostics)?;
    match serde_json::from_slice(&bytes) {
        Ok(document) => Some((document, bytes)),
        Err(error) => {
            parse_error(path, "JSON", &error.to_string(), diagnostics);
            None
        }
    }
}

fn load_regular_blob(
    root: &Path,
    tree: &str,
    path: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Vec<u8>> {
    let relative = Path::new(path);
    if !manifest_file::repository_relative(relative) {
        candidate_error(path, "candidate actor path is not repository-relative", diagnostics);
        return None;
    }
    let output =
        command(root, &["ls-tree", "-z", "--full-tree", tree, "--", path], relative, diagnostics)?;
    if !output.status.success() {
        candidate_error(path, "candidate tree entry cannot be inspected", diagnostics);
        return None;
    }
    let Some(object_id) = exact_regular_blob_entry(&output.stdout, path.as_bytes()) else {
        candidate_error(
            path,
            "candidate actor policy is missing, non-regular, or not an exact blob path",
            diagnostics,
        );
        return None;
    };
    let output = command(root, &["cat-file", "blob", object_id], relative, diagnostics)?;
    if !output.status.success() {
        candidate_error(path, "candidate actor policy blob cannot be read", diagnostics);
        return None;
    }
    Some(output.stdout)
}

fn exact_regular_blob_entry<'a>(output: &'a [u8], expected_path: &[u8]) -> Option<&'a str> {
    let mut records = output.split(|byte| *byte == 0).filter(|record| !record.is_empty());
    let record = records.next()?;
    if records.next().is_some() {
        return None;
    }
    let tab = record.iter().position(|byte| *byte == b'\t')?;
    if &record[tab + 1..] != expected_path {
        return None;
    }
    let metadata = std::str::from_utf8(&record[..tab]).ok()?;
    let mut fields = metadata.split(' ');
    let mode = fields.next()?;
    let kind = fields.next()?;
    let object_id = fields.next()?;
    if fields.next().is_some()
        || !matches!(mode, "100644" | "100755")
        || kind != "blob"
        || !full_object_id(object_id)
    {
        return None;
    }
    Some(object_id)
}

fn full_object_id(value: &str) -> bool {
    value.len() == 40
        && value.bytes().all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        && value.bytes().any(|byte| byte != b'0')
}

fn command(
    root: &Path,
    arguments: &[&str],
    path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Output> {
    Command::new("git").args(arguments).current_dir(root).output().map_or_else(
        |error| {
            candidate_error(
                path,
                format!("could not inspect candidate actor Git binding: {error}"),
                diagnostics,
            );
            None
        },
        Some,
    )
}

fn parse_error(path: &str, format: &str, detail: &str, diagnostics: &mut Vec<Diagnostic>) {
    candidate_error(
        path,
        format!("candidate actor policy does not match its {format} schema: {detail}"),
        diagnostics,
    );
}

fn candidate_error(
    path: impl AsRef<Path>,
    message: impl Into<String>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    diagnostics.push(Diagnostic::at(
        path.as_ref(),
        message,
        "restore the exact append-only candidate actor registry in the reviewed implementation tree",
    ));
}

#[cfg(test)]
mod tests {
    use super::exact_regular_blob_entry;

    #[test]
    fn tree_entry_parser_accepts_only_one_exact_regular_blob() {
        let object = "1234567890abcdef1234567890abcdef12345678";
        let regular = format!("100644 blob {object}\tverification/actors.toml\0");
        assert_eq!(
            exact_regular_blob_entry(regular.as_bytes(), b"verification/actors.toml"),
            Some(object)
        );
        for record in [
            format!("120000 blob {object}\tverification/actors.toml\0"),
            format!("040000 tree {object}\tverification/actors.toml\0"),
            format!("100644 blob {object}\tverification/other.toml\0"),
            format!(
                "100644 blob {object}\tverification/actors.toml\0100644 blob {object}\tverification/actors.toml\0"
            ),
        ] {
            assert!(
                exact_regular_blob_entry(record.as_bytes(), b"verification/actors.toml").is_none()
            );
        }
    }
}

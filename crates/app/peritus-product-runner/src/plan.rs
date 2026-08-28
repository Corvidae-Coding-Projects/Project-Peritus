//! Checked JSON file-plan parsing and transactional application.

use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
};

use serde::Deserialize;

use crate::{ProductRunnerError, ProductRunnerErrorKind};

const MAX_CHANGED_FILES: usize = 128;
const MAX_PLAN_BYTES: usize = 8 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FilePlan {
    summary: String,
    #[serde(default)]
    files: Vec<FileReplacement>,
    #[serde(default)]
    deletions: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileReplacement {
    path: String,
    content: String,
}

#[derive(Debug)]
pub struct AppliedPlan {
    pub summary: String,
    pub changed_files: usize,
}

pub fn apply(root: &Path, response: &str) -> Result<AppliedPlan, ProductRunnerError> {
    let json = extract_json(response)?;
    if json.len() > MAX_PLAN_BYTES {
        return Err(invalid("model file plan exceeds its size bound"));
    }
    let plan: FilePlan = serde_json::from_str(json).map_err(|error| {
        ProductRunnerError::new(
            ProductRunnerErrorKind::InvalidModelOutput,
            "parse model file plan",
            error.to_string(),
        )
    })?;
    if plan.summary.trim().is_empty() || plan.files.len() + plan.deletions.len() > MAX_CHANGED_FILES
    {
        return Err(invalid("model file plan is empty or changes too many files"));
    }
    let replacements = plan
        .files
        .into_iter()
        .map(|file| checked_path(root, &file.path).map(|path| (path, file.content.into_bytes())))
        .collect::<Result<Vec<_>, _>>()?;
    let deletions = plan
        .deletions
        .iter()
        .map(|path| checked_path(root, path))
        .collect::<Result<Vec<_>, _>>()?;
    let mut backups = BTreeMap::new();
    for path in replacements.iter().map(|(path, _)| path).chain(deletions.iter()) {
        backups.entry(path.clone()).or_insert_with(|| fs::read(path).ok());
    }
    if let Err(error) = apply_all(&replacements, &deletions) {
        rollback(&backups);
        return Err(error);
    }
    Ok(AppliedPlan { summary: plan.summary, changed_files: replacements.len() + deletions.len() })
}

fn apply_all(
    replacements: &[(PathBuf, Vec<u8>)],
    deletions: &[PathBuf],
) -> Result<(), ProductRunnerError> {
    for (path, content) in replacements {
        if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err(invalid("model file plan cannot replace a symbolic link"));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| apply_error(&error))?;
        }
        let temporary = path.with_extension("peritus-new");
        fs::write(&temporary, content).map_err(|error| apply_error(&error))?;
        replace_file(&temporary, path)?;
    }
    for path in deletions {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(apply_error(&error)),
        }
    }
    Ok(())
}

fn replace_file(temporary: &Path, target: &Path) -> Result<(), ProductRunnerError> {
    #[cfg(windows)]
    if target.is_file() {
        fs::remove_file(target).map_err(|error| apply_error(&error))?;
    }
    fs::rename(temporary, target).map_err(|error| apply_error(&error))
}

fn rollback(backups: &BTreeMap<PathBuf, Option<Vec<u8>>>) {
    for (path, content) in backups {
        match content {
            Some(content) => {
                let _ = fs::write(path, content);
            }
            None => {
                let _ = fs::remove_file(path);
            }
        }
    }
}

fn checked_path(root: &Path, value: &str) -> Result<PathBuf, ProductRunnerError> {
    let relative = Path::new(value);
    if value.is_empty()
        || relative.is_absolute()
        || relative.components().any(|component| !matches!(component, Component::Normal(_)))
        || relative.starts_with(".git")
    {
        return Err(invalid("model file plan contains an unsafe relative path"));
    }
    Ok(root.join(relative))
}

fn extract_json(value: &str) -> Result<&str, ProductRunnerError> {
    let start = value.find('{').ok_or_else(|| invalid("model response contains no JSON object"))?;
    let end = value
        .rfind('}')
        .ok_or_else(|| invalid("model response contains no complete JSON object"))?;
    if end < start {
        return Err(invalid("model response contains malformed JSON"));
    }
    Ok(&value[start..=end])
}

fn invalid(detail: &'static str) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidModelOutput,
        "validate model file plan",
        detail,
    )
}

fn apply_error(error: &std::io::Error) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Apply,
        "apply model file plan",
        error.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_replacement_plan_applies_and_reports_changes() {
        let temporary = tempfile::tempdir().expect("temporary workspace");
        fs::write(temporary.path().join("old.txt"), "old").expect("fixture");
        let plan = r#"{"summary":"implemented","files":[{"path":"src/new.rs","content":"pub fn answer() -> u8 { 42 }\n"}],"deletions":["old.txt"]}"#;
        let applied = apply(temporary.path(), plan).expect("apply plan");
        assert_eq!(applied.summary, "implemented");
        assert_eq!(applied.changed_files, 2);
        assert_eq!(
            fs::read_to_string(temporary.path().join("src/new.rs")).unwrap(),
            "pub fn answer() -> u8 { 42 }\n"
        );
        assert!(!temporary.path().join("old.txt").exists());
    }

    #[test]
    fn traversal_and_git_metadata_are_rejected() {
        let temporary = tempfile::tempdir().expect("temporary workspace");
        for path in ["../escape", ".git/config"] {
            let plan = format!(
                r#"{{"summary":"bad","files":[{{"path":"{path}","content":"x"}}],"deletions":[]}}"#
            );
            assert_eq!(
                apply(temporary.path(), &plan).expect_err("unsafe path rejects").kind(),
                ProductRunnerErrorKind::InvalidModelOutput
            );
        }
    }
}

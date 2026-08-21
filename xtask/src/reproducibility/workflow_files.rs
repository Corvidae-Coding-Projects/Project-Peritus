use crate::error::XtaskError;
use std::fs;
use std::path::{Path, PathBuf};

const WORKFLOW_DIRECTORY: &str = ".github/workflows";
pub(super) const ACTION_DIRECTORY: &str = ".github/actions";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum DocumentKind {
    Workflow,
    Action,
}

pub(super) fn workflow_files(root: &Path) -> Result<Vec<(PathBuf, DocumentKind)>, XtaskError> {
    let directory = root.join(WORKFLOW_DIRECTORY);
    let mut files = fs::read_dir(&directory)
        .map_err(|error| XtaskError::io("read directory", &directory, error))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| XtaskError::io("read directory entry in", &directory, error))
        })
        .collect::<Result<Vec<_>, _>>()?;
    files.retain(|path| path.is_file() && has_yaml_extension(path));
    Ok(files.into_iter().map(|path| (path, DocumentKind::Workflow)).collect())
}

pub(super) fn action_files(root: &Path) -> Result<Vec<(PathBuf, DocumentKind)>, XtaskError> {
    let directory = root.join(ACTION_DIRECTORY);
    if !directory.exists() {
        return Ok(Vec::new());
    }
    let mut pending = vec![directory];
    let mut files = Vec::new();
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(&current)
            .map_err(|error| XtaskError::io("read directory", &current, error))?
        {
            let entry = entry
                .map_err(|error| XtaskError::io("read directory entry in", &current, error))?;
            let file_type = entry
                .file_type()
                .map_err(|error| XtaskError::io("read file type for", &entry.path(), error))?;
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if (file_type.is_file() || file_type.is_symlink() && entry.path().is_file())
                && is_action_manifest(&entry.path())
            {
                files.push((entry.path(), DocumentKind::Action));
            }
        }
    }
    Ok(files)
}

fn has_yaml_extension(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "yml" || extension == "yaml")
}

fn is_action_manifest(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "action.yml" || name == "action.yaml")
}

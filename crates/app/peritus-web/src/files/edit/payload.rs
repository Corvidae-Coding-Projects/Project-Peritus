//! Exact save bodies survive browser and gateway restart under their original effect identity.
use crate::{error::{Result, problem}, state::App};
use std::path::PathBuf;

fn legacy_path(app: &App, operation: &str) -> Result<PathBuf> {
    crate::operations::validate_identity(operation)?;
    let parent = app.options.state_file.parent().ok_or_else(|| problem("Workspace state has no directory"))?;
    let identity = crate::files::revision(operation.as_bytes());
    Ok(parent.join("operation-payloads").join(format!("{identity}.text")))
}
fn path(app: &App, operation: &str) -> Result<PathBuf> {
    let legacy = legacy_path(app, operation)?;
    let parent = legacy.parent().ok_or_else(|| problem("Save payload has no directory"))?;
    let workspace = crate::files::revision(app.snapshot()?.identity.as_bytes());
    let name = legacy.file_name().ok_or_else(|| problem("Save payload has no identity"))?;
    Ok(parent.join(workspace).join(name))
}
pub(super) fn directory(app: &App, operation: &str) -> Result<PathBuf> {
    path(app, operation)?.parent().map(std::path::Path::to_owned)
        .ok_or_else(|| problem("Save payload has no directory"))
}
pub(super) fn retain(app: &App, operation: &str, body: super::source::Body) -> Result<PathBuf> {
    let path = path(app, operation)?;
    let expected = (body.hash.clone(), body.bytes);
    if super::source::inspect(body.temporary.path())? != expected {
        return Err(problem("The original save body changed before durable retention"));
    }
    let parent = path.parent().ok_or_else(|| problem("Save payload has no directory"))?;
    std::fs::create_dir_all(parent)?;
    match body.temporary.persist_noclobber(&path) {
        Ok(file) => { file.sync_all()?; },
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
            if super::source::inspect(&path)? != expected {
                return Err(problem("The original save identity already owns different file contents"));
            }
        },
        Err(error) => return Err(problem(error)),
    }
    #[cfg(unix)]
    std::fs::File::open(parent).and_then(|directory| directory.sync_all())
        .map_err(crate::error::uncertain)?;
    Ok(path)
}
pub(super) fn restore_path(app: &App, operation: &str, expected: &str) -> Result<PathBuf> {
    let path = available_path(app, operation)?
        .ok_or_else(|| problem("The exact original save body is not retained by this gateway"))?;
    if super::source::inspect(&path)?.0 != expected {
        return Err(problem("The retained save body failed its exact digest check"));
    }
    Ok(path)
}
fn available_path(app: &App, operation: &str) -> Result<Option<PathBuf>> {
    let path = path(app, operation)?;
    match std::fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_file() => return Ok(Some(path)),
        Ok(_) => return Err(problem("The retained save body is not a regular file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
        Err(error) => return Err(error.into()),
    }
    // Legacy bodies may be recovered only through an authoritative file-save record in
    // this workspace's operation store; a same-named file alone conveys no ownership.
    if app.operation(operation)?.is_none_or(|record| record.input["command"] != "file-save") {
        return Ok(None);
    }
    let legacy = legacy_path(app, operation)?;
    match std::fs::symlink_metadata(&legacy) {
        Ok(metadata) if metadata.is_file() => Ok(Some(legacy)),
        Ok(_) => Err(problem("The legacy retained save body is not a regular file")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
pub(super) fn restore_optional(app: &App, operation: &str, expected: &str) -> Result<Option<String>> {
    let Some(path) = available_path(app, operation)? else { return Ok(None) };
    let bytes = std::fs::read(path)?;
    if super::revision(&bytes) != expected { return Err(problem("The retained save body failed its exact digest check")) }
    super::validate_text(&bytes)?;
    String::from_utf8(bytes).map(Some).map_err(problem)
}

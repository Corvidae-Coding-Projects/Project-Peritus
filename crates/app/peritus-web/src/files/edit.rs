//! Optimistic, atomic text saves with exact unresolved payloads retained separately from receipts.

mod payload;
mod source;
pub(crate) use source::inspect;

use super::{read_text, resolve, revision, validate_text};
use crate::{
    error::{Result, problem},
    state::{App, OperationOwner, Publication},
};
use axum::{
    Json,
    extract::{Query, Request, State},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{io::Write, path::Path, sync::Arc};

#[derive(Deserialize)]
pub struct SaveArgs {
    project: String,
    path: String,
    revision: String,
    operation: String,
    workspace: String,
}

pub async fn save(
    State(app): State<Arc<App>>,
    Query(args): Query<SaveArgs>,
    request: Request,
) -> Result<Json<Value>> {
    if args.workspace != app.snapshot()?.identity {
        return Err(problem("The file save belongs to another gateway workspace; retain its original body and reconcile it there"));
    }
    crate::operations::validate_identity(&args.operation)?;
    if args.revision.len() != 64
        || !args.revision.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err(problem("A valid original revision and operation identity are required"));
    }
    let root = app.project(&args.project)?.root;
    let target = resolve(&root, &args.path)?;
    let body = source::receive(payload::directory(&app, &args.operation)?, request).await?;
    let hash = body.hash.clone();
    let bytes = body.bytes;
    let input = json!({"command":"file-save","project":args.project,"path":args.path,"revision":args.revision,"hash":hash});
    let lock = app.lock(format!("operation:{}", args.operation))?;
    let operation_guard = lock.lock_owned().await;
    let owner = app.own_operation(&args.operation).await?;
    let file_lock = app.lock(format!("file:{}", target.display()))?;
    let file_guard = file_lock.lock_owned().await;
    // The worker owns both admission guards and the durable receipt until settlement.
    // Dropping an HTTP observation cannot release ownership of an accepted replacement.
    let result = tokio::task::spawn_blocking(move || -> Result<Value> {
        let _operation_guard = operation_guard;
        let _file_guard = file_guard;
        let complete = || -> Result<Value> {
            if let Some(prior) = owner.get()? {
                if prior.input != input {
                    return Err(problem("This operation identity belongs to a different save"));
                }
                if let Some(result) = prior.result { return Ok(result) }
                // A crash may occur after rename but before the receipt. Only observe.
                if inspect(&target)? == (hash.clone(), bytes) {
                    let result = json!({"revision":hash,"bytes":bytes,"recovered":true});
                    owner.settle(result.clone()).map_err(|error| crate::error::uncertain(error.0))?;
                    return Ok(result);
                }
                return Ok(json!({"error":"The interrupted save could not be confirmed. Reload the disk version before saving again.","uncertain":true}));
            }
            let retained = payload::retain(&app, &args.operation, body)?;
            if owner.insert(input)? == Publication::Existing {
                return Err(crate::error::uncertain("The save receipt changed while its exact owner was retained"));
            }
            let result = match source::replace(&root, &args.path, &args.revision, &retained, &hash, bytes) {
                Ok(result) => result,
                Err(error) if error.1 => return Ok(json!({"error":error.0,"uncertain":true})),
                Err(error) => json!({"error":error.0,"retryable":true,"submitted":false}),
            };
            owner.settle(result.clone()).map_err(|error| crate::error::uncertain(error.0))?;
            Ok(result)
        };
        let result = complete();
        if let Err(error) = &result {
            eprintln!("peritus web: owned file save {}: {error}", args.operation);
        }
        result
    }).await.map_err(crate::error::uncertain)??;
    Ok(Json(result))
}

pub(super) fn write(root: &Path, relative: &str, expected: &str, bytes: &[u8]) -> Result<Value> {
    validate_text(bytes)?;
    let path = resolve(root, relative)?;
    let metadata = std::fs::symlink_metadata(root.join(relative))?;
    if !metadata.is_file() || metadata.permissions().readonly() {
        return Err(problem("Editing requires a writable regular file, not a symbolic link"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if metadata.nlink() > 1 {
            return Err(problem("Editing a hard-linked file requires an external editor"));
        }
    }
    if revision(&read_text(&path)?) != expected {
        return Err(problem(
            "The file changed on disk. Your draft is retained. Reload the disk version or copy your changes before trying again.",
        ));
    }
    let parent = path.parent().ok_or_else(|| problem("No parent directory"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.as_file().set_permissions(metadata.permissions())?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    if resolve(root, relative)? != path || revision(&read_text(&path)?) != expected {
        return Err(problem(
            "The file changed while saving. Your draft is retained; reload the disk version.",
        ));
    }
    temporary.persist(&path).map_err(problem)?;
    // The file data is durable before replacement; sync its directory on Unix as well.
    #[cfg(unix)]
    std::fs::File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(crate::error::uncertain)?;
    Ok(json!({"revision":revision(bytes),"bytes":bytes.len()}))
}

pub(crate) struct RetryOwner {
    pub(crate) receipt: OperationOwner,
    pub(crate) operation: tokio::sync::OwnedMutexGuard<()>,
    pub(crate) resource: tokio::sync::OwnedMutexGuard<()>,
}

/// Explicit retry retains the original file preimage, body and effect ownership to settlement.
pub(crate) async fn retry(app: Arc<App>, ownership: RetryOwner, original: Value, proposed: Value) -> Result<Value> {
    let root = app.project(original["project"].as_str().unwrap_or(""))?.root;
    let path = original["path"].as_str().unwrap_or("").to_owned();
    let target = resolve(&root, &path)?;
    let lock = app.lock(format!("file:{}", target.display()))?;
    let file_guard = lock.lock_owned().await;
    tokio::task::spawn_blocking(move || -> Result<Value> {
        let _file_guard = file_guard;
        let RetryOwner { receipt: owner, operation: _operation_guard, resource: _resource_guard } = ownership;
        let hash = original["hash"].as_str().ok_or_else(|| problem("The original save has no exact body digest"))?;
        let retained = if let Some(text) = proposed["text"].as_str() {
            if proposed["project"] != original["project"] || proposed["path"] != original["path"]
                || proposed["revision"] != original["revision"] || revision(text.as_bytes()) != hash
            {
                return Err(problem("The retry must contain the original save body and preimage"));
            }
            let body = source::from_text(&payload::directory(&app, owner.identity())?, text)?;
            payload::retain(&app, owner.identity(), body)?
        } else {
            payload::restore_path(&app, owner.identity(), hash)?
        };
        let (retained_hash, bytes) = inspect(&retained)?;
        if retained_hash != hash {
            return Err(problem("The retained save body differs from the original operation"));
        }
        let result = match source::replace(&root, &path, original["revision"].as_str().unwrap_or(""), &retained, hash, bytes) {
            Ok(result) => result,
            Err(error) if error.1 => return Err(error),
            Err(error) => json!({"error":error.0,"retryable":true,"submitted":false}),
        };
        owner.complete_retry(result.clone()).map_err(|error| crate::error::uncertain(error.0))?;
        Ok(result)
    }).await.map_err(crate::error::uncertain)?
}

/// Returns an original save body independently of whether its file replacement was accepted.
pub fn retained(app: &App, operation: &str, input: &Value) -> Result<Option<Value>> {
    Ok(payload::restore_optional(app, operation, input["hash"].as_str().unwrap_or(""))?
        .map(|text| json!({"project":input["project"],"path":input["path"],"revision":input["revision"],"text":text})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn saves_exact_utf8_and_rejects_stale_versions() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("notes.md");
        let initial = b"\xef\xbb\xbftypo\r\n";
        std::fs::write(&path, initial).unwrap();
        write(root.path(), "notes.md", &revision(initial), b"\xef\xbb\xbfcorrect\r\n").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"\xef\xbb\xbfcorrect\r\n");
        assert!(write(root.path(), "notes.md", &revision(initial), b"lost change").is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"\xef\xbb\xbfcorrect\r\n");
        assert!(write(root.path(), "notes.md", "bad", &[0]).is_err());
    }
    #[test]
    fn text_reads_cross_the_former_preview_ceiling() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("large.txt");
        std::fs::write(&path, vec![b'a'; 3 * 1024 * 1024]).unwrap();
        assert!(super::super::text(root.path(), "large.txt").is_ok());
        let large = vec![b'x'; 51 * 1024 * 1024];
        std::fs::write(&path, &large).unwrap();
        assert_eq!(read_text(&path).unwrap(), large);
        assert!(read_text(root.path()).is_err());
    }
    #[test]
    #[cfg(unix)]
    fn preserves_permissions_and_does_not_replace_links() {
        use std::os::unix::fs::PermissionsExt;
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("script.sh");
        std::fs::write(&path, b"old").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o750)).unwrap();
        write(root.path(), "script.sh", &revision(b"old"), b"new").unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o750);
        std::os::unix::fs::symlink(&path, root.path().join("link")).unwrap();
        assert!(write(root.path(), "link", &revision(b"new"), b"bad").is_err());
    }
}

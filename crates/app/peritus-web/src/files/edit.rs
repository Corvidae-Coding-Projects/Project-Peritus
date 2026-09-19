//! Optimistic, atomic text saves. The journal holds hashes, never whole file bodies.

use super::{read_text, resolve, revision, validate_text};
use crate::{
    error::{Result, problem},
    state::{App, Operation},
};
use axum::{
    Json,
    body::Bytes,
    extract::{Query, State},
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
}

pub async fn save(
    State(app): State<Arc<App>>,
    Query(args): Query<SaveArgs>,
    bytes: Bytes,
) -> Result<Json<Value>> {
    if args.operation.is_empty()
        || args.operation.len() > 80
        || !args.operation.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        || args.revision.len() != 64
    {
        return Err(problem("A valid original revision and operation identity are required"));
    }
    validate_text(&bytes)?;
    let root = app.project(&args.project)?.root;
    let target = resolve(&root, &args.path)?;
    let hash = revision(&bytes);
    let input = json!({"command":"file-save","project":args.project,"path":args.path,"revision":args.revision,"hash":hash});
    let lock = app.lock(format!("operation:{}", args.operation))?;
    let _operation_guard = lock.lock().await;
    let file_lock = app.lock(format!("file:{}", target.display()))?;
    let _file_guard = file_lock.lock().await;
    if let Some(prior) = app.snapshot()?.operations.get(&args.operation) {
        if prior.input != input {
            return Err(problem("This operation identity belongs to a different save"));
        }
        if let Some(result) = &prior.result {
            return Ok(Json(result.clone()));
        }
        // A crash may occur after rename but before the receipt. Only observe; never rewrite.
        let result = if revision(&read_text(&target)?) == hash {
            json!({"revision":hash,"bytes":bytes.len(),"recovered":true})
        } else {
            json!({"error":"The interrupted save could not be confirmed. Reload the disk version before saving again.","uncertain":true})
        };
        return Ok(Json(result));
    }
    app.update(|state| {
        state.operations.insert(args.operation.clone(), Operation { input, result: None });
        Ok(())
    })?;
    let result =
        tokio::task::spawn_blocking(move || write(&root, &args.path, &args.revision, &bytes))
            .await
            .map_err(problem)?;
    let result = match result {
        Ok(value) => value,
        Err(error) if error.1 => return Ok(Json(json!({"error":error.0,"uncertain":true}))),
        Err(error) => json!({"error":error.0}),
    };
    app.update(|state| {
        state
            .operations
            .get_mut(&args.operation)
            .ok_or_else(|| problem("Save receipt missing"))?
            .result = Some(result.clone());
        Ok(())
    })
    .map_err(|error| crate::error::uncertain(error.0))?;
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
    fn larger_previews_are_bounded() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("large.txt");
        std::fs::write(&path, vec![b'a'; 3 * 1024 * 1024]).unwrap();
        assert!(super::super::text(root.path(), "large.txt").is_ok());
        std::fs::write(&path, vec![b'x'; super::super::TEXT_LIMIT]).unwrap();
        assert_eq!(read_text(&path).unwrap().len(), super::super::TEXT_LIMIT);
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len((super::super::TEXT_LIMIT + 1) as u64)
            .unwrap();
        assert!(read_text(&path).is_err());
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

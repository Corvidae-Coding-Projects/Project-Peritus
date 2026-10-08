//! Physical save windows preserve the complete logical body and its exact digest.

use crate::{error::{Result, problem}, state::hex};
use axum::extract::Request;
use futures_util::StreamExt;
use sha2::{Digest, Sha256};
use std::{fs::File, io::{Read, Write}, path::Path};
use tokio::io::AsyncWriteExt;

const WINDOW: usize = 64 * 1024;

pub(super) struct Body {
    pub(super) temporary: tempfile::NamedTempFile,
    pub(super) hash: String,
    pub(super) bytes: u64,
}

struct Text {
    pending: Vec<u8>,
}

impl Text {
    fn new() -> Self { Self { pending: Vec::with_capacity(WINDOW + 3) } }

    fn push(&mut self, bytes: &[u8]) -> Result<()> {
        if bytes.contains(&0) {
            return Err(problem("This file contains binary data and cannot be edited as text"));
        }
        self.pending.extend_from_slice(bytes);
        let validated = match std::str::from_utf8(&self.pending) {
            Ok(_) => self.pending.len(),
            Err(error) if error.error_len().is_none() => error.valid_up_to(),
            Err(error) => return Err(problem(error)),
        };
        self.pending.drain(..validated);
        Ok(())
    }

    fn finish(self) -> Result<()> {
        if self.pending.is_empty() { return Ok(()) }
        Err(problem("The text body ends inside a UTF-8 character"))
    }
}

pub(super) async fn receive(directory: std::path::PathBuf, request: Request) -> Result<Body> {
    let temporary = tokio::task::spawn_blocking(move || -> Result<_> {
        std::fs::create_dir_all(&directory)?;
        Ok(tempfile::NamedTempFile::new_in(directory)?)
    }).await.map_err(problem)??;
    let file = temporary.reopen()?;
    let mut file = tokio::fs::File::from_std(file);
    let mut stream = request.into_body().into_data_stream();
    let mut text = Text::new();
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(problem)?;
        for window in chunk.chunks(WINDOW) {
            text.push(window)?;
            bytes = bytes.checked_add(u64::try_from(window.len()).map_err(problem)?)
                .ok_or_else(|| problem("The save body exceeds its byte-count representation"))?;
            digest.update(window);
            file.write_all(window).await?;
        }
    }
    text.finish()?;
    file.flush().await?;
    file.sync_all().await?;
    drop(file);
    Ok(Body { temporary, hash: hex(&digest.finalize()), bytes })
}

pub(super) fn from_text(directory: &Path, value: &str) -> Result<Body> {
    std::fs::create_dir_all(directory)?;
    let mut temporary = tempfile::NamedTempFile::new_in(directory)?;
    let mut text = Text::new();
    let mut digest = Sha256::new();
    for window in value.as_bytes().chunks(WINDOW) {
        text.push(window)?;
        digest.update(window);
        temporary.write_all(window)?;
    }
    text.finish()?;
    temporary.as_file().sync_all()?;
    Ok(Body { temporary, hash: hex(&digest.finalize()), bytes: u64::try_from(value.len()).map_err(problem)? })
}

/// Revalidates retained text without allocating its cumulative length.
pub(crate) fn inspect(path: &Path) -> Result<(String, u64)> {
    let mut file = File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(problem("Choose a regular text file"));
    }
    let mut window = [0_u8; WINDOW];
    let mut text = Text::new();
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    loop {
        let count = file.read(&mut window)?;
        if count == 0 { break }
        text.push(&window[..count])?;
        bytes = bytes.checked_add(u64::try_from(count).map_err(problem)?)
            .ok_or_else(|| problem("The text file exceeds its byte-count representation"))?;
        digest.update(&window[..count]);
    }
    text.finish()?;
    Ok((hex(&digest.finalize()), bytes))
}

pub(super) fn replace(
    root: &Path, relative: &str, expected: &str, body: &Path, hash: &str, bytes: u64,
) -> Result<serde_json::Value> {
    let path = super::resolve(root, relative)?;
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
    if inspect(&path)?.0 != expected {
        return Err(problem("The file changed on disk. Your draft is retained. Reload the disk version or copy your changes before trying again."));
    }
    let parent = path.parent().ok_or_else(|| problem("No parent directory"))?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.as_file().set_permissions(metadata.permissions())?;
    let mut retained = File::open(body)?;
    std::io::copy(&mut retained, &mut temporary)?;
    temporary.as_file().sync_all()?;
    if inspect(temporary.path())? != (hash.to_owned(), bytes) {
        return Err(problem("The retained save body changed before publication"));
    }
    if super::resolve(root, relative)? != path || inspect(&path)?.0 != expected {
        return Err(problem("The file changed while saving. Your draft is retained; reload the disk version."));
    }
    temporary.persist(&path).map_err(problem)?;
    #[cfg(unix)]
    File::open(parent).and_then(|directory| directory.sync_all())
        .map_err(crate::error::uncertain)?;
    Ok(serde_json::json!({"revision":hash,"bytes":bytes}))
}

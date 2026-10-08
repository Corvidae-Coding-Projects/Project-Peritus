//! Incremental text observation with a verified full-source editing preimage.

use crate::error::{Result, problem};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use tokio::io::AsyncReadExt;

const READ_BYTES: usize = 64 * 1024;

#[cfg(windows)]
mod identity;

enum Phase { Read, Verify, Complete }

pub(crate) struct TextSource {
    file: tokio::fs::File,
    root: PathBuf,
    relative: String,
    path: PathBuf,
    initial: std::fs::Metadata,
    phase: Phase,
    pending: Vec<u8>,
    bytes: u64,
    verified: u64,
    digest: Sha256,
    verification: Sha256,
}

impl TextSource {
    pub(crate) async fn open(root: PathBuf, relative: String) -> Result<Self> {
        let selected_root=root.clone();
        let selected_relative=relative.clone();
        let path=tokio::task::spawn_blocking(move || super::resolve(&selected_root, &selected_relative))
            .await.map_err(problem)??;
        let file=tokio::fs::File::open(&path).await?;
        let initial=file.metadata().await?;
        if !initial.is_file() { return Err(problem("Choose a regular text file")); }
        Ok(Self {
            file, root, relative, path, initial, phase: Phase::Read,
            pending: Vec::new(), bytes: 0, verified: 0,
            digest: Sha256::new(), verification: Sha256::new(),
        })
    }

    pub(crate) async fn next(&mut self) -> Result<Option<Value>> {
        let mut window=vec![0_u8;READ_BYTES];
        loop {
            match self.phase {
                Phase::Read => {
                    let count=self.file.read(&mut window).await?;
                    if count==0 {
                        if !self.pending.is_empty() { return Err(problem("The file ends with incomplete UTF-8 text. Use Download.")); }
                        // Verify the currently named file, rather than only the old open
                        // descriptor: an atomic replacement must participate in the preimage.
                        let selected_root=self.root.clone();
                        let selected_relative=self.relative.clone();
                        let current=tokio::task::spawn_blocking(move || super::resolve(&selected_root,&selected_relative))
                            .await.map_err(problem)??;
                        if current!=self.path { return Err(problem("The file changed while reading. Reload its current version; retained drafts remain available.")); }
                        self.file=tokio::fs::File::open(current).await?;
                        self.phase=Phase::Verify;
                        return Ok(Some(json!({"verifying":"0","bytes":self.bytes.to_string()})));
                    }
                    self.digest.update(&window[..count]);
                    self.pending.extend_from_slice(&window[..count]);
                    if self.pending.contains(&0) { return Err(problem("Binary content cannot be edited as text. Use Download.")); }
                    let valid=match std::str::from_utf8(&self.pending) {
                        Ok(_) => self.pending.len(),
                        Err(error) if error.error_len().is_none() => error.valid_up_to(),
                        Err(_) => return Err(problem("Non-UTF-8 content cannot be edited as text. Use Download.")),
                    };
                    if valid==0 { continue; }
                    let text=std::str::from_utf8(&self.pending[..valid]).map_err(problem)?.to_owned();
                    let offset=self.bytes;
                    self.bytes=self.bytes.checked_add(u64::try_from(valid).map_err(problem)?)
                        .ok_or_else(|| problem("File byte offset overflowed"))?;
                    self.pending.drain(..valid);
                    return Ok(Some(json!({"offset":offset.to_string(),"text":text})));
                }
                Phase::Verify => {
                    let count=self.file.read(&mut window).await?;
                    if count>0 {
                        self.verification.update(&window[..count]);
                        self.verified=self.verified.checked_add(u64::try_from(count).map_err(problem)?)
                            .ok_or_else(|| problem("File byte offset overflowed"))?;
                        return Ok(Some(json!({"verifying":self.verified.to_string(),"bytes":self.bytes.to_string()})));
                    }
                    let first=self.digest.clone().finalize();
                    if self.verified!=self.bytes || self.verification.clone().finalize()!=first {
                        return Err(problem("The file changed while reading. Reload its current version; retained drafts remain available."));
                    }
                    let selected_root=self.root.clone();
                    let selected_relative=self.relative.clone();
                    let current=tokio::task::spawn_blocking(move || super::resolve(&selected_root,&selected_relative))
                        .await.map_err(problem)??;
                    let after=self.file.metadata().await?;
                    let named_file=tokio::fs::File::open(&current).await?;
                    let named=named_file.metadata().await?;
                    if current!=self.path || after.len()!=self.initial.len()
                        || after.modified().ok()!=self.initial.modified().ok()
                        || named.len()!=after.len() || named.modified().ok()!=after.modified().ok()
                        || !same_file(&self.file,&named_file).await?
                    {
                        return Err(problem("The file changed while reading. Reload its current version; retained drafts remain available."));
                    }
                    self.phase=Phase::Complete;
                    return Ok(Some(json!({"complete":true,"revision":crate::state::hex(&first),"bytes":self.bytes.to_string()})));
                }
                Phase::Complete => return Ok(None),
            }
        }
    }
}

#[cfg(unix)]
async fn same_file(left:&tokio::fs::File,right:&tokio::fs::File)->Result<bool> {
    use std::os::unix::fs::MetadataExt as _;
    let left=left.metadata().await?;
    let right=right.metadata().await?;
    Ok(left.dev()==right.dev() && left.ino()==right.ino())
}

#[cfg(windows)]
async fn same_file(left:&tokio::fs::File,right:&tokio::fs::File)->Result<bool> {
    let left=left.try_clone().await?.into_std().await;
    let right=right.try_clone().await?.into_std().await;
    tokio::task::spawn_blocking(move || Ok(identity::file(&left)?==identity::file(&right)?))
        .await.map_err(problem)?
}

//! No-clobber retained evidence written from direct controller observations.

use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::digest;

#[derive(Serialize)]
pub(super) struct EvidenceDocument {
    kind: &'static str,
    id: &'static str,
    path: &'static str,
    sha256: String,
    bytes: u64,
}

pub(super) struct EvidenceSet {
    root: PathBuf,
    documents: Vec<EvidenceDocument>,
    bytes: u64,
    maximum_bytes: u32,
}

impl EvidenceSet {
    pub(super) fn new(root: &Path, maximum_bytes: u32) -> Self {
        Self { root: root.to_path_buf(), documents: Vec::with_capacity(6), bytes: 0, maximum_bytes }
    }

    pub(super) fn retain<T: Serialize>(
        &mut self,
        kind: &'static str,
        id: &'static str,
        path: &'static str,
        observation: &T,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut bytes = serde_json::to_vec_pretty(observation)?;
        bytes.push(b'\n');
        let next = self
            .bytes
            .checked_add(u64::try_from(bytes.len())?)
            .ok_or("H1 evidence byte accounting overflowed")?;
        if next > u64::from(self.maximum_bytes) {
            return Err("H1 retained evidence exceeds the request byte limit".into());
        }
        let target = self.root.join(path);
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options.open(&target)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        let metadata = fs::symlink_metadata(&target)?;
        if !metadata.file_type().is_file() || metadata.len() != bytes.len() as u64 {
            return Err("H1 retained evidence is not the exact regular file written".into());
        }
        let sha256 = digest::hex(digest::file(&target)?);
        self.documents.push(EvidenceDocument { kind, id, path, sha256, bytes: metadata.len() });
        self.bytes = next;
        Ok(())
    }

    pub(super) fn finish(self) -> Result<(Vec<EvidenceDocument>, u16, u32), &'static str> {
        if self.documents.len() != 6 {
            return Err("H1 production recovery requires exactly six evidence classes");
        }
        let count = u16::try_from(self.documents.len())
            .map_err(|_| "H1 evidence count exceeded its protocol bound")?;
        let bytes = u32::try_from(self.bytes)
            .map_err(|_| "H1 evidence bytes exceeded their protocol bound")?;
        Ok((self.documents, count, bytes))
    }
}

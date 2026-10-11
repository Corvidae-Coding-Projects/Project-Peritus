//! Incremental private staging with source hashes and exact retained write progress.

use super::{
    BundleExportOperation, BundleLimits, BundlePlan, BundleReceipt,
    assemble::{StagedBundle, ensure_capacity},
    format::{MAGIC, invalid},
};
use crate::{EvidenceCancellation, EvidenceError, artifact::ArtifactInput};
use peritus_artifact_store::ArtifactStore;
use peritus_types::Sha256Digest;
use sha2::{Digest, Sha256};
use std::{
    io::{self, Write},
    num::NonZeroUsize,
    path::Path,
};
use tempfile::NamedTempFile;

#[derive(Clone, Copy)]
enum Section {
    Magic,
    Manifest,
    RecordCount,
    Record,
    FrameCount,
    Frame,
    ArtifactCount,
    ArtifactHeader,
    Artifact,
    Root,
    Complete,
}

/// Owns private bundle staging, artifact hashes, and partial output across pauses and failures.
///
/// No caller output is exposed until every source artifact and the exact planned length have
/// passed verification. Dropping this operation removes its private temporary file.
pub struct BundlePreparation<'a> {
    plan: &'a BundlePlan,
    artifacts: &'a ArtifactStore,
    staged: NamedTempFile,
    section: Section,
    index: usize,
    pending: Vec<u8>,
    offset: usize,
    artifact: Option<ArtifactInput>,
    hash: Sha256,
    written: u64,
    receipt: Option<BundleReceipt>,
}

impl<'a> BundlePreparation<'a> {
    /// Preflights exact encoded limits and creates a private stage on the artifact filesystem.
    ///
    /// # Errors
    /// Returns selected-budget, representation, free-space, or staging I/O failures.
    pub fn new(
        plan: &'a BundlePlan,
        artifacts: &'a ArtifactStore,
        limits: BundleLimits,
    ) -> Result<Self, EvidenceError> {
        Self::new_in(artifacts.root(), plan, artifacts, limits)
    }
    pub(super) fn new_in(
        directory: &Path,
        plan: &'a BundlePlan,
        artifacts: &'a ArtifactStore,
        limits: BundleLimits,
    ) -> Result<Self, EvidenceError> {
        plan.validate_limits(limits)?;
        ensure_capacity(directory, plan.byte_count())?;
        let staged = NamedTempFile::new_in(directory)
            .map_err(|error| EvidenceError::io("create staged evidence bundle", error))?;
        Ok(Self {
            plan,
            artifacts,
            staged,
            section: Section::Magic,
            index: 0,
            pending: Vec::new(),
            offset: 0,
            artifact: None,
            hash: Sha256::new(),
            written: 0,
            receipt: None,
        })
    }
    /// Returns exact bytes accepted by the private stage.
    #[must_use]
    pub const fn staged_bytes(&self) -> u64 {
        self.written
    }
    /// Returns exact source bytes hashed in the current artifact.
    #[must_use]
    pub fn artifact_offset(&self) -> u64 {
        self.artifact.as_ref().map_or(0, ArtifactInput::offset)
    }

    /// Writes at most `bytes` bytes to the private stage, retaining any partially written chunk.
    ///
    /// Artifact reads use at most 64 KiB and are never rehashed after a successful step. A fresh
    /// cancellation token resumes the retained owner. The completed receipt authenticates all
    /// staged bytes; the caller may then obtain a resumable output operation.
    ///
    /// # Errors
    /// Returns cancellation, changed source bytes, selected-limit, or staging I/O failures.
    pub fn advance(
        &mut self,
        bytes: NonZeroUsize,
        cancellation: &EvidenceCancellation,
    ) -> Result<Option<BundleReceipt>, EvidenceError> {
        let mut budget = bytes.get();
        loop {
            cancellation.check("prepare evidence bundle")?;
            if self.receipt.is_some() {
                return Ok(self.receipt);
            }
            if self.offset < self.pending.len() {
                if budget == 0 {
                    return Ok(None);
                }
                let count = budget.min(self.pending.len() - self.offset);
                let written = match self
                    .staged
                    .as_file_mut()
                    .write(&self.pending[self.offset..self.offset + count])
                {
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(error) => {
                        return Err(EvidenceError::io("write staged evidence bundle", error));
                    }
                    Ok(0) => {
                        return Err(EvidenceError::io(
                            "write staged evidence bundle",
                            io::ErrorKind::WriteZero.into(),
                        ));
                    }
                    Ok(written) => written,
                };
                self.hash.update(&self.pending[self.offset..self.offset + written]);
                self.offset += written;
                self.written += written as u64;
                budget -= written;
                continue;
            }
            self.pending.clear();
            self.offset = 0;
            if matches!(self.section, Section::Complete) {
                if self.written != self.plan.byte_count() {
                    return Err(invalid("staged length disagrees with exact plan"));
                }
                self.staged.as_file_mut().flush().and_then(|()| self.staged.as_file().sync_all())
                    .map_err(|error| EvidenceError::io("synchronize staged evidence bundle", error))?;
                self.receipt = Some(BundleReceipt {
                    export_id: self.plan.export_id(),
                    root_digest: self.plan.manifest().root_digest(),
                    bundle_digest: Sha256Digest::new(self.hash.clone().finalize().into()),
                    byte_count: self.written,
                });
                return Ok(self.receipt);
            }
            if budget == 0 {
                return Ok(None);
            }
            self.next_segment(budget)?;
        }
    }

    /// Transfers the authenticated private stage into a retained output operation.
    ///
    /// # Errors
    /// Rejects a preparation that has not completed every source check and synchronization.
    pub fn into_export(self) -> Result<BundleExportOperation, EvidenceError> {
        Ok(BundleExportOperation::from_staged(self.into_staged()?))
    }
    pub(super) fn into_staged(self) -> Result<StagedBundle, EvidenceError> {
        Ok(StagedBundle {
            file: self.staged,
            receipt: self.receipt.ok_or_else(|| invalid("bundle preparation is incomplete"))?,
        })
    }

    fn next_segment(&mut self, budget: usize) -> Result<(), EvidenceError> {
        match self.section {
            Section::Magic => {
                self.pending.extend_from_slice(MAGIC);
                self.section = Section::Manifest;
            }
            Section::Manifest => {
                self.pending
                    .extend_from_slice(&self.plan.manifest().canonical_size()?.to_be_bytes());
                self.plan.manifest().write_canonical(&mut |bytes| {
                    self.pending.extend_from_slice(bytes);
                    Ok(())
                })?;
                self.section = Section::RecordCount;
            }
            Section::RecordCount => {
                self.pending.extend_from_slice(&(self.plan.records().len() as u64).to_be_bytes());
                self.index = 0;
                self.section = Section::Record;
            }
            Section::Record => {
                if let Some(record) = self.plan.records().get(self.index) {
                    self.pending.extend_from_slice(&record.canonical_size()?.to_be_bytes());
                    record.write_canonical(&mut |bytes| {
                        self.pending.extend_from_slice(bytes);
                        Ok(())
                    })?;
                    self.index += 1;
                } else {
                    self.section = Section::FrameCount;
                }
            }
            Section::FrameCount => {
                self.pending.extend_from_slice(&(self.plan.frames().len() as u64).to_be_bytes());
                self.index = 0;
                self.section = Section::Frame;
            }
            Section::Frame => {
                if let Some(frame) = self.plan.frames().get(self.index) {
                    self.pending.extend_from_slice(&frame.entry.global_position().to_be_bytes());
                    self.pending.extend_from_slice(&(frame.bytes.len() as u64).to_be_bytes());
                    self.pending.extend_from_slice(&frame.bytes);
                    self.index += 1;
                } else {
                    self.section = Section::ArtifactCount;
                }
            }
            Section::ArtifactCount => {
                self.pending.extend_from_slice(
                    &(self.plan.manifest().artifacts().len() as u64).to_be_bytes(),
                );
                self.index = 0;
                self.section = Section::ArtifactHeader;
            }
            Section::ArtifactHeader => {
                if let Some(entry) = self.plan.manifest().artifacts().get(self.index) {
                    let input = ArtifactInput::open(self.artifacts, entry.digest())?;
                    if input.size() != entry.size() {
                        return Err(invalid("artifact size changed after planning"));
                    }
                    self.artifact = Some(input);
                    self.pending.extend_from_slice(entry.digest().as_bytes());
                    self.pending.extend_from_slice(&entry.size().to_be_bytes());
                    self.section = Section::Artifact;
                } else {
                    self.section = Section::Root;
                }
            }
            Section::Artifact => {
                let input =
                    self.artifact.as_mut().ok_or_else(|| invalid("artifact source is absent"))?;
                self.pending.resize(budget.min(64 * 1024), 0);
                let read = match input.read(&mut self.pending) {
                    Ok(read) => read,
                    Err(error) => {
                        self.pending.clear();
                        return Err(error);
                    }
                };
                self.pending.truncate(read);
                if read == 0 {
                    self.artifact = None;
                    self.index += 1;
                    self.section = Section::ArtifactHeader;
                }
            }
            Section::Root => {
                self.pending.extend_from_slice(self.plan.manifest().root_digest().as_bytes());
                self.section = Section::Complete;
            }
            Section::Complete => {}
        }
        Ok(())
    }
}

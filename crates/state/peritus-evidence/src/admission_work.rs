//! Owned admission continuation, with exact retries before dependency work.

use crate::{
    EvidenceCancellation, EvidenceDraft, EvidenceError, EvidenceRecord, EvidenceStore,
    artifact::ArtifactInput,
};
use peritus_artifact_store::ArtifactStore;
use peritus_journal::IntegrityExport;
use std::num::NonZeroUsize;

/// Owns one exact draft, immutable export snapshot, verified artifact cursor, and catalog owner.
///
/// No writer transaction is held while artifact bytes are read. Exact committed retries finish
/// before consulting the export or artifact objects; new admissions recheck their durable journal,
/// causal, identity, and referenceability fences atomically at commit.
pub struct EvidenceAdmission<'a> {
    store: &'a mut EvidenceStore,
    draft: EvidenceDraft,
    export: &'a IntegrityExport,
    artifacts: &'a ArtifactStore,
    next_artifact: usize,
    current: Option<ArtifactInput>,
    receipt: Option<EvidenceRecord>,
    checked_retry: bool,
}

impl<'a> EvidenceAdmission<'a> {
    /// Takes ownership of a draft and borrows its exact dependencies without doing I/O.
    #[must_use]
    pub const fn new(
        store: &'a mut EvidenceStore,
        draft: EvidenceDraft,
        export: &'a IntegrityExport,
        artifacts: &'a ArtifactStore,
    ) -> Self {
        Self {
            store,
            draft,
            export,
            artifacts,
            next_artifact: 0,
            current: None,
            receipt: None,
            checked_retry: false,
        }
    }
    /// Returns the stable identity retained across cancellation and retry.
    #[must_use]
    pub const fn id(&self) -> crate::EvidenceId {
        self.draft.id()
    }
    /// Returns completed artifact verifications.
    #[must_use]
    pub const fn verified_artifacts(&self) -> usize {
        self.next_artifact
    }
    /// Returns bytes already hashed in the currently open artifact.
    #[must_use]
    pub fn artifact_offset(&self) -> u64 {
        self.current.as_ref().map_or(0, ArtifactInput::offset)
    }

    /// Hashes at most the caller-selected step size and commits once all dependencies are checked.
    ///
    /// Cancellation or storage contention retains the immutable draft, completed hashes, and
    /// current open artifact. `None` means more work remains, not admission failure.
    ///
    /// # Errors
    /// Returns cancellation, I/O, corrupt artifact, provenance, identity, or catalog failures.
    pub fn advance(
        &mut self,
        bytes: NonZeroUsize,
        cancellation: &EvidenceCancellation,
    ) -> Result<Option<&EvidenceRecord>, EvidenceError> {
        cancellation.check("advance evidence admission")?;
        if self.receipt.is_some() {
            return Ok(self.receipt.as_ref());
        }
        if !self.checked_retry {
            self.receipt = self.store.committed_retry(&self.draft)?;
            if self.receipt.is_some() {
                return Ok(self.receipt.as_ref());
            }
            self.checked_retry = true;
        }
        if self.next_artifact < self.draft.artifacts().len() {
            if self.current.is_none() {
                self.current = Some(ArtifactInput::open(
                    self.artifacts,
                    self.draft.artifacts()[self.next_artifact],
                )?);
            }
            let mut buffer = vec![0_u8; bytes.get().min(64 * 1024)].into_boxed_slice();
            if let Some(input) = self.current.as_mut()
                && input.read(&mut buffer)? == 0
            {
                self.next_artifact += 1;
                self.current = None;
            }
            cancellation.check("advance evidence admission")?;
            return Ok(None);
        }
        cancellation.check("commit evidence admission")?;
        self.receipt = Some(self.store.commit_admission(self.draft.clone(), self.export)?);
        Ok(self.receipt.as_ref())
    }

    /// Drains bounded steps with the supplied cancellation owner.
    ///
    /// # Errors
    /// Returns the same failures as [`Self::advance`].
    pub fn finish(
        &mut self,
        cancellation: &EvidenceCancellation,
    ) -> Result<EvidenceRecord, EvidenceError> {
        let budget = const { NonZeroUsize::new(64 * 1024).unwrap() };
        loop {
            if let Some(record) = self.advance(budget, cancellation)? {
                return Ok(record.clone());
            }
        }
    }
}

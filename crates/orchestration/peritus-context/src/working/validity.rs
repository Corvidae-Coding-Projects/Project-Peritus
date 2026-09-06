//! Explicit workspace dependencies and conservative freshness decisions.

use super::{WorkingBinding, WorkingError, WorkingLimits};
use crate::ContextNodeId;
use peritus_types::Sha256Digest;
use vstd::prelude::*;

verus! {
/// Digest of one host-identified file or entity, without ambient path or filesystem access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkingFileDigest {
    key: ContextNodeId,
    digest: Sha256Digest,
}

impl WorkingFileDigest {
    /// Records a caller-verified file/entity digest.
    #[must_use]
    pub const fn new(key: ContextNodeId, digest: Sha256Digest) -> Self { Self { key, digest } }
    /// Stable entity key supplied by the host's source index.
    #[must_use]
    pub const fn key(self) -> ContextNodeId { self.key }
    /// Exact content revision.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest { self.digest }
}

/// Current checked environment used to invalidate dependent conclusions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingEnvironment {
    binding: WorkingBinding,
    candidate: Sha256Digest,
    files: Vec<WorkingFileDigest>,
}

impl WorkingEnvironment {
    /// Creates a canonical bounded environment observation.
    ///
    /// # Errors
    /// Rejects unordered, duplicate, or excessive entity keys.
    pub fn new(
        binding: WorkingBinding,
        candidate: Sha256Digest,
        files: Vec<WorkingFileDigest>,
        limits: WorkingLimits,
    ) -> Result<Self, WorkingError> {
        validate_files(&files, limits.entries())?;
        Ok(Self { binding, candidate, files })
    }
    /// Current scope and conversation revision.
    #[must_use]
    pub const fn binding(&self) -> WorkingBinding { self.binding }
    /// Current complete candidate digest.
    #[must_use]
    pub const fn candidate(&self) -> Sha256Digest { self.candidate }
    /// Canonical observed file/entity digests.
    #[must_use]
    pub const fn files(&self) -> &[WorkingFileDigest] { self.files.as_slice() }
}

/// Explicit validity dependencies; an uncertain dependency set always requires reinspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingValidity {
    conversation_revision: Option<u64>,
    candidate: Option<Sha256Digest>,
    files: Vec<WorkingFileDigest>,
    requires_recheck: bool,
}

impl WorkingValidity {
    /// Binds a conclusion to exact known dependencies. All supplied constraints must hold.
    ///
    /// Empty constraints explicitly mean task-local lifetime. Use `uncertain` when dependencies
    /// are unknown; an omitted observed file never counts as an unchanged file.
    ///
    /// # Errors
    /// Rejects excessive, unordered, or repeated entity keys.
    pub fn new(
        conversation_revision: Option<u64>,
        candidate: Option<Sha256Digest>,
        files: Vec<WorkingFileDigest>,
        limits: WorkingLimits,
    ) -> Result<Self, WorkingError> {
        validate_files(&files, limits.links())?;
        Ok(Self { conversation_revision, candidate, files, requires_recheck: false })
    }

    /// Requires a fresh check before a conclusion may become current.
    #[must_use]
    pub const fn uncertain() -> Self {
        Self { conversation_revision: None, candidate: None, files: Vec::new(), requires_recheck: true }
    }
    /// Optional exact user-conversation dependency.
    #[must_use]
    pub const fn conversation_revision(&self) -> Option<u64> { self.conversation_revision }
    /// Optional exact complete-candidate dependency.
    #[must_use]
    pub const fn candidate(&self) -> Option<Sha256Digest> { self.candidate }
    /// Exact file/entity dependencies.
    #[must_use]
    pub const fn files(&self) -> &[WorkingFileDigest] { self.files.as_slice() }
    /// Whether unknown dependencies prohibit reuse without a fresh check.
    #[must_use]
    pub const fn requires_recheck(&self) -> bool { self.requires_recheck }

    /// Evaluates all constraints against host-supplied observations, without changing status.
    #[must_use]
    pub fn holds(&self, environment: &WorkingEnvironment) -> bool {
        if self.requires_recheck { return false; }
        if matches!(self.conversation_revision, Some(revision) if revision != environment.binding.conversation_revision()) { return false; }
        if matches!(self.candidate, Some(candidate) if candidate != environment.candidate) { return false; }
        let mut index = 0;
        while index < self.files.len()
            invariant index <= self.files.len(),
            decreases self.files.len() - index,
        {
            let file = self.files[index];
            if !has_file(environment.files(), file) { return false; }
            index += 1;
        }
        true
    }
}

fn has_file(files: &[WorkingFileDigest], expected: WorkingFileDigest) -> bool {
    let mut index = 0;
    while index < files.len()
        invariant index <= files.len(),
        decreases files.len() - index,
    {
        if files[index] == expected { return true; }
        index += 1;
    }
    false
}

fn validate_files(files: &[WorkingFileDigest], maximum: usize) -> Result<(), WorkingError> {
    if files.len() > maximum { return Err(WorkingError::Capacity); }
    let mut index = 1;
    while index < files.len()
        invariant index >= 1,
        decreases files.len() - index,
    {
        if files[index - 1].key >= files[index].key {
            return Err(WorkingError::NonCanonicalOrder);
        }
        index += 1;
    }
    Ok(())
}
}

//! Explicit workspace dependencies and conservative freshness decisions.

use super::{WorkingBinding, WorkingError, WorkingLimits};
use crate::ContextNodeId;
use peritus_types::Sha256Digest;

#[doc = " Digest of one host-identified file or entity, without ambient path or filesystem access."]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkingFileDigest {
    key: ContextNodeId,
    digest: Sha256Digest,
}
impl WorkingFileDigest {
    #[doc = " Records a caller-verified file/entity digest."]
    #[must_use]
    pub const fn new(key: ContextNodeId, digest: Sha256Digest) -> Self {
        Self { key, digest }
    }
    #[doc = " Stable entity key supplied by the host\'s source index."]
    #[must_use]
    pub const fn key(self) -> ContextNodeId {
        self.key
    }
    #[doc = " Exact content revision."]
    #[must_use]
    pub const fn digest(self) -> Sha256Digest {
        self.digest
    }
    pub(super) fn matches(self, other: Self) -> bool {
        self.key.matches(&other.key) && digest_matches(self.digest, other.digest)
    }
}
#[doc = " Current checked environment used to invalidate dependent conclusions."]
#[derive(Debug, Eq, PartialEq)]
pub struct WorkingEnvironment {
    binding: WorkingBinding,
    candidate: Sha256Digest,
    files: Vec<WorkingFileDigest>,
}
impl Clone for WorkingEnvironment {
    fn clone(&self) -> Self {
        Self { binding: self.binding, candidate: self.candidate, files: self.files.clone() }
    }
}
impl WorkingEnvironment {
    #[doc = " Creates a canonical bounded environment observation."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Rejects unordered, duplicate, or excessive entity keys."]
    pub fn new(
        binding: WorkingBinding,
        candidate: Sha256Digest,
        files: Vec<WorkingFileDigest>,
        limits: WorkingLimits,
    ) -> Result<Self, WorkingError> {
        validate_files(&files, limits.entries())?;
        Ok(Self { binding, candidate, files })
    }
    #[doc = " Current scope and conversation revision."]
    #[must_use]
    pub const fn binding(&self) -> WorkingBinding {
        self.binding
    }
    #[doc = " Current complete candidate digest."]
    #[must_use]
    pub const fn candidate(&self) -> Sha256Digest {
        self.candidate
    }
    #[doc = " Canonical observed file/entity digests."]
    #[must_use]
    pub const fn files(&self) -> &[WorkingFileDigest] {
        self.files.as_slice()
    }
}
#[doc = " Explicit validity dependencies; an uncertain dependency set always requires reinspection."]
#[derive(Debug, Eq, PartialEq)]
pub struct WorkingValidity {
    conversation_revision: Option<u64>,
    candidate: Option<Sha256Digest>,
    files: Vec<WorkingFileDigest>,
    requires_recheck: bool,
}
impl WorkingValidity {
    #[doc = " Binds a conclusion to exact known dependencies. All supplied constraints must hold."]
    #[doc = ""]
    #[doc = " Empty constraints explicitly mean task-local lifetime. Use `uncertain` when dependencies"]
    #[doc = " are unknown; an omitted observed file never counts as an unchanged file."]
    #[doc = ""]
    #[doc = " # Errors"]
    #[doc = " Rejects excessive, unordered, or repeated entity keys."]
    pub fn new(
        conversation_revision: Option<u64>,
        candidate: Option<Sha256Digest>,
        files: Vec<WorkingFileDigest>,
        limits: WorkingLimits,
    ) -> Result<Self, WorkingError> {
        validate_files(&files, limits.links())?;
        Ok(Self { conversation_revision, candidate, files, requires_recheck: false })
    }
    #[doc = " Requires a fresh check before a conclusion may become current."]
    #[must_use]
    pub const fn uncertain() -> Self {
        Self {
            conversation_revision: None,
            candidate: None,
            files: Vec::new(),
            requires_recheck: true,
        }
    }
    #[doc = " Optional exact user-conversation dependency."]
    #[must_use]
    pub const fn conversation_revision(&self) -> Option<u64> {
        self.conversation_revision
    }
    #[doc = " Optional exact complete-candidate dependency."]
    #[must_use]
    pub const fn candidate(&self) -> Option<Sha256Digest> {
        self.candidate
    }
    #[doc = " Exact file/entity dependencies."]
    #[must_use]
    pub const fn files(&self) -> &[WorkingFileDigest] {
        self.files.as_slice()
    }
    #[doc = " Whether unknown dependencies prohibit reuse without a fresh check."]
    #[must_use]
    pub const fn requires_recheck(&self) -> bool {
        self.requires_recheck
    }
    #[doc = " Evaluates all constraints against host-supplied observations, without changing status."]
    #[must_use]
    pub fn holds(&self, environment: &WorkingEnvironment) -> bool {
        if self.requires_recheck {
            return false;
        }
        if matches!
        (self.conversation_revision, Some(revision) if revision !=
        environment.binding.conversation_revision())
        {
            return false;
        }
        if matches!
        (self.candidate, Some(candidate) if !
        digest_matches(candidate, environment.candidate))
        {
            return false;
        }
        let mut index = 0;
        while index < self.files.len() {
            let file = self.files[index];
            if !has_file(environment.files(), file) {
                return false;
            }
            index += 1;
        }
        true
    }
}
impl Clone for WorkingValidity {
    fn clone(&self) -> Self {
        Self {
            conversation_revision: self.conversation_revision,
            candidate: self.candidate,
            files: self.files.clone(),
            requires_recheck: self.requires_recheck,
        }
    }
}
fn has_file(files: &[WorkingFileDigest], expected: WorkingFileDigest) -> bool {
    let mut index = 0;
    while index < files.len() {
        if files[index].matches(expected) {
            return true;
        }
        index += 1;
    }
    false
}
const fn digest_matches(left: Sha256Digest, right: Sha256Digest) -> bool {
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let mut index = 0;
    while index < left_bytes.len() {
        if left_bytes[index] != right_bytes[index] {
            return false;
        }
        index += 1;
    }
    true
}
fn validate_files(files: &[WorkingFileDigest], maximum: usize) -> Result<(), WorkingError> {
    if files.len() > maximum {
        return Err(WorkingError::Capacity);
    }
    let mut index = 1;
    while index < files.len() {
        if files[index - 1].key >= files[index].key {
            return Err(WorkingError::NonCanonicalOrder);
        }
        index += 1;
    }
    Ok(())
}

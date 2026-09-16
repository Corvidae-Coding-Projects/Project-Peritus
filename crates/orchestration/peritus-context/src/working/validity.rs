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
    /// Logical stable entity identity.
    pub closed spec fn spec_key(self) -> ContextNodeId { self.key }
    /// Logical exact entity digest.
    pub closed spec fn spec_digest(self) -> Sha256Digest { self.digest }
    /// Exact identity and digest equality.
    pub open spec fn spec_matches(self, other: Self) -> bool {
        self.spec_key().spec_matches(&other.spec_key())
            && self.spec_digest().spec_bytes() == other.spec_digest().spec_bytes()
    }
    /// Records a caller-verified file/entity digest.
    #[must_use]
    pub const fn new(key: ContextNodeId, digest: Sha256Digest) -> Self { Self { key, digest } }
    /// Stable entity key supplied by the host's source index.
    #[must_use]
    pub const fn key(self) -> ContextNodeId { self.key }
    /// Exact content revision.
    #[must_use]
    pub const fn digest(self) -> Sha256Digest { self.digest }

    pub(super) fn matches(self, other: Self) -> (matches: bool)
        ensures matches == self.spec_matches(other),
    {
        self.key.matches(&other.key) && digest_matches(self.digest, other.digest)
    }
}

/// Current checked environment used to invalidate dependent conclusions.
#[derive(Debug, Eq, PartialEq)]
pub struct WorkingEnvironment {
    binding: WorkingBinding,
    candidate: Sha256Digest,
    files: Vec<WorkingFileDigest>,
}

impl Clone for WorkingEnvironment {
    fn clone(&self) -> (result: Self)
        ensures
            result.spec_binding() == self.spec_binding(),
            result.spec_candidate() == self.spec_candidate(),
            result.spec_files() == self.spec_files(),
    {
        Self {
            binding: self.binding,
            candidate: self.candidate,
            files: self.files.clone(),
        }
    }
}

impl WorkingEnvironment {
    /// Logical scope and conversation binding.
    pub closed spec fn spec_binding(&self) -> WorkingBinding { self.binding }
    /// Logical complete candidate digest.
    pub closed spec fn spec_candidate(&self) -> Sha256Digest { self.candidate }
    /// Logical canonical file/entity revisions.
    pub closed spec fn spec_files(&self) -> Seq<WorkingFileDigest> { self.files@ }

    /// Creates a canonical bounded environment observation.
    ///
    /// # Errors
    /// Rejects unordered, duplicate, or excessive entity keys.
    pub fn new(
        binding: WorkingBinding,
        candidate: Sha256Digest,
        files: Vec<WorkingFileDigest>,
        limits: WorkingLimits,
    ) -> (result: Result<Self, WorkingError>)
        ensures match result {
            Ok(environment) => {
                &&& environment.spec_binding() == binding
                &&& environment.spec_candidate() == candidate
                &&& environment.spec_files() == files@
            }
            Err(_) => true,
        },
    {
        validate_files(&files, limits.entries())?;
        Ok(Self { binding, candidate, files })
    }
    /// Current scope and conversation revision.
    #[must_use]
    pub const fn binding(&self) -> (result: WorkingBinding)
        ensures result == self.spec_binding(),
    { self.binding }
    /// Current complete candidate digest.
    #[must_use]
    pub const fn candidate(&self) -> (result: Sha256Digest)
        ensures result == self.spec_candidate(),
    { self.candidate }
    /// Canonical observed file/entity digests.
    #[must_use]
    pub const fn files(&self) -> (result: &[WorkingFileDigest])
        ensures result@ == self.spec_files(),
    { self.files.as_slice() }
}

/// Explicit validity dependencies; an uncertain dependency set always requires reinspection.
#[derive(Debug, Eq, PartialEq)]
pub struct WorkingValidity {
    conversation_revision: Option<u64>,
    candidate: Option<Sha256Digest>,
    files: Vec<WorkingFileDigest>,
    requires_recheck: bool,
}

impl WorkingValidity {
    /// Logical optional conversation revision dependency.
    pub closed spec fn spec_conversation_revision(&self) -> Option<u64> {
        self.conversation_revision
    }
    /// Logical optional complete-candidate dependency.
    pub closed spec fn spec_candidate(&self) -> Option<Sha256Digest> { self.candidate }
    /// Logical canonical file dependency sequence.
    pub closed spec fn spec_files(&self) -> Seq<WorkingFileDigest> { self.files@ }
    /// Logical unknown-dependency marker.
    pub closed spec fn spec_requires_recheck(&self) -> bool { self.requires_recheck }
    /// Complete semantic equality retained by cloning validity constraints.
    pub open spec fn clone_equivalent(left: &Self, right: &Self) -> bool {
        left.spec_conversation_revision() == right.spec_conversation_revision()
            && left.spec_candidate() == right.spec_candidate()
            && left.spec_files() == right.spec_files()
            && left.spec_requires_recheck() == right.spec_requires_recheck()
    }
    /// Exact evaluation against a host-observed environment.
    pub open spec fn spec_holds(&self, environment: &WorkingEnvironment) -> bool {
        &&& !self.spec_requires_recheck()
        &&& match self.spec_conversation_revision() {
            Some(revision) => revision == environment.spec_binding().spec_conversation_revision(),
            None => true,
        }
        &&& match self.spec_candidate() {
            Some(candidate) => candidate.spec_bytes()
                == environment.spec_candidate().spec_bytes(),
            None => true,
        }
        &&& forall |index: int| #![auto] 0 <= index < self.spec_files().len() ==>
            spec_has_file(environment.spec_files(), self.spec_files()[index])
    }
    pub(super) proof fn clone_equivalent_holds(
        left: &Self,
        right: &Self,
        environment: &WorkingEnvironment,
    )
        requires Self::clone_equivalent(left, right),
        ensures left.spec_holds(environment) == right.spec_holds(environment),
    {
        reveal(WorkingValidity::clone_equivalent);
        reveal(WorkingValidity::spec_holds);
    }
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
    pub fn holds(&self, environment: &WorkingEnvironment) -> (holds: bool)
        ensures holds == self.spec_holds(environment),
    {
        if self.requires_recheck { return false; }
        if matches!(self.conversation_revision, Some(revision) if revision != environment.binding.conversation_revision()) { return false; }
        if matches!(self.candidate, Some(candidate) if !digest_matches(candidate, environment.candidate)) { return false; }
        let mut index = 0;
        while index < self.files.len()
            invariant
                index <= self.files.len(),
                forall |prior: int| #![auto] 0 <= prior < index ==>
                    spec_has_file(environment.spec_files(), self.spec_files()[prior]),
            decreases self.files.len() - index,
        {
            let file = self.files[index];
            if !has_file(environment.files(), file) {
                assert(self.spec_files()[index as int] == file);
                assert(!spec_has_file(environment.spec_files(), self.spec_files()[index as int]));
                assert(!self.spec_holds(environment));
                return false;
            }
            index += 1;
        }
        true
    }
}

impl Clone for WorkingValidity {
    fn clone(&self) -> (result: Self)
        ensures Self::clone_equivalent(self, &result),
    {
        Self {
            conversation_revision: self.conversation_revision,
            candidate: self.candidate,
            files: self.files.clone(),
            requires_recheck: self.requires_recheck,
        }
    }
}

/// Whether an exact entity identity and digest occurs in an observed file sequence.
pub open spec fn spec_has_file(
    files: Seq<WorkingFileDigest>,
    expected: WorkingFileDigest,
) -> bool {
    exists |index: int| 0 <= index < files.len()
        && files[index].spec_matches(expected)
}

fn has_file(files: &[WorkingFileDigest], expected: WorkingFileDigest) -> (found: bool)
    ensures found == spec_has_file(files@, expected),
{
    let mut index = 0;
    while index < files.len()
        invariant
            index <= files.len(),
            forall |prior: int| #![auto] 0 <= prior < index ==>
                !files@[prior].spec_matches(expected),
        decreases files.len() - index,
    {
        if files[index].matches(expected) { return true; }
        index += 1;
    }
    false
}

const fn digest_matches(left: Sha256Digest, right: Sha256Digest) -> (matches: bool)
    ensures matches == (left.spec_bytes() == right.spec_bytes()),
{
    let left_bytes = left.as_bytes();
    let right_bytes = right.as_bytes();
    let mut index = 0;
    while index < left_bytes.len()
        invariant
            index <= left_bytes.len(),
            *left_bytes == left.spec_bytes(),
            *right_bytes == right.spec_bytes(),
            forall |prior: int| 0 <= prior < index ==>
                left_bytes@[prior] == right_bytes@[prior],
        decreases left_bytes.len() - index,
    {
        if left_bytes[index] != right_bytes[index] {
            assert(left.spec_bytes()[index as int] != right.spec_bytes()[index as int]);
            return false;
        }
        index += 1;
    }
    assert(*left_bytes =~= *right_bytes);
    true
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

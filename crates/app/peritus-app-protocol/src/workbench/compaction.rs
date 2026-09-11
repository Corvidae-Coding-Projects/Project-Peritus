//! Content-free deterministic prompt-view previews over immutable public replies.

use crate::{AppErrorCode, AppProtocolError, WorkbenchInvocationId, WorkbenchQuery};
use peritus_types::Sha256Digest;

/// Maximum UTF-8 bytes in an optional user compaction focus.
pub const MAX_WORKBENCH_COMPACTION_FOCUS_BYTES: usize = 1024;
/// Maximum source handles in one bounded deterministic preview.
pub const MAX_WORKBENCH_COMPACTION_ENTRIES: usize = 1024;

const fn invalid() -> AppProtocolError {
    AppProtocolError::new(AppErrorCode::MalformedFrame, None)
}

/// Bounded inert user preference for deterministic local compaction.
#[derive(Clone, Eq, PartialEq)]
pub struct WorkbenchCompactionFocus(String);
impl WorkbenchCompactionFocus {
    /// Validates a nonempty bounded focus without terminal control characters.
    ///
    /// # Errors
    /// Rejects empty, oversized, or terminal-control-containing text.
    pub fn new(value: String) -> Result<Self, AppProtocolError> {
        if value.trim().is_empty()
            || value.len() > MAX_WORKBENCH_COMPACTION_FOCUS_BYTES
            || value.chars().any(|character| character.is_control() && character != '\n')
        {
            return Err(invalid());
        }
        Ok(Self(value))
    }
    /// Borrows the exact user preference.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for WorkbenchCompactionFocus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WorkbenchCompactionFocus")
            .field("bytes", &self.0.len())
            .finish_non_exhaustive()
    }
}

/// Revision-fenced request for a local deterministic prompt-view proposal.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchCompactionRequest {
    query: WorkbenchQuery,
    revision: u64,
    focus: Option<WorkbenchCompactionFocus>,
}
impl WorkbenchCompactionRequest {
    /// Creates an exact, non-current-alias request.
    ///
    /// # Errors
    /// Rejects revision zero because a proposal must bind a state the user can confirm.
    pub fn new(
        query: WorkbenchQuery,
        revision: u64,
        focus: Option<WorkbenchCompactionFocus>,
    ) -> Result<Self, AppProtocolError> {
        if revision == 0 {
            return Err(invalid());
        }
        Ok(Self { query, revision, focus })
    }
    /// Returns conversation and workspace scope.
    #[must_use]
    pub const fn query(&self) -> WorkbenchQuery {
        self.query
    }
    /// Returns the exact aggregate revision inspected.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
    /// Borrows the optional exact user focus.
    #[must_use]
    pub const fn focus(&self) -> Option<&WorkbenchCompactionFocus> {
        self.focus.as_ref()
    }
}

/// One immutable source and its proposed smaller deterministic handle.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchCompactionEntry {
    invocation: WorkbenchInvocationId,
    source_digest: Sha256Digest,
    source_bytes: u64,
    replacement_digest: Sha256Digest,
    replacement_bytes: u64,
}
impl WorkbenchCompactionEntry {
    /// Validates a strictly smaller nonempty replacement.
    ///
    /// # Errors
    /// Rejects empty or non-saving entries.
    pub const fn new(
        invocation: WorkbenchInvocationId,
        source_digest: Sha256Digest,
        source_bytes: u64,
        replacement_digest: Sha256Digest,
        replacement_bytes: u64,
    ) -> Result<Self, AppProtocolError> {
        if replacement_bytes == 0 || replacement_bytes >= source_bytes {
            return Err(invalid());
        }
        Ok(Self { invocation, source_digest, source_bytes, replacement_digest, replacement_bytes })
    }
    /// Returns the invocation preceding the immutable public reply.
    #[must_use]
    pub const fn invocation(self) -> WorkbenchInvocationId {
        self.invocation
    }
    /// Returns the exact immutable source digest.
    #[must_use]
    pub const fn source_digest(self) -> Sha256Digest {
        self.source_digest
    }
    /// Returns the exact immutable source size.
    #[must_use]
    pub const fn source_bytes(self) -> u64 {
        self.source_bytes
    }
    /// Returns the deterministic replacement digest.
    #[must_use]
    pub const fn replacement_digest(self) -> Sha256Digest {
        self.replacement_digest
    }
    /// Returns the deterministic replacement size.
    #[must_use]
    pub const fn replacement_bytes(self) -> u64 {
        self.replacement_bytes
    }
}

/// Exact proposed prompt-view generation; applying it requires a second revision-fenced command.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchCompactionPreview {
    request: WorkbenchCompactionRequest,
    generation: u64,
    source_bytes: u64,
    replacement_bytes: u64,
    recent_preserved: u32,
    pinned_preserved: u32,
    unresolved_preserved: u32,
    unsavable_preserved: u32,
    entries: Vec<WorkbenchCompactionEntry>,
}
impl WorkbenchCompactionPreview {
    /// Validates aggregate sizes, canonical unique source order, and bounds.
    ///
    /// # Errors
    /// Rejects empty, unsorted, inconsistent, or non-saving proposals.
    pub fn new(
        request: WorkbenchCompactionRequest,
        generation: u64,
        recent_preserved: u32,
        pinned_preserved: u32,
        unresolved_preserved: u32,
        unsavable_preserved: u32,
        entries: Vec<WorkbenchCompactionEntry>,
    ) -> Result<Self, AppProtocolError> {
        if generation == 0
            || entries.len() > MAX_WORKBENCH_COMPACTION_ENTRIES
            || [recent_preserved, pinned_preserved, unresolved_preserved, unsavable_preserved]
                .into_iter()
                .any(|count| count as usize > MAX_WORKBENCH_COMPACTION_ENTRIES)
            || entries
                .windows(2)
                .any(|pair| pair[0].invocation().as_bytes() >= pair[1].invocation().as_bytes())
        {
            return Err(invalid());
        }
        let source_bytes =
            entries.iter().try_fold(0_u64, |total, entry| total.checked_add(entry.source_bytes()));
        let replacement_bytes = entries
            .iter()
            .try_fold(0_u64, |total, entry| total.checked_add(entry.replacement_bytes()));
        let (Some(source_bytes), Some(replacement_bytes)) = (source_bytes, replacement_bytes)
        else {
            return Err(invalid());
        };
        if (!entries.is_empty() && replacement_bytes >= source_bytes)
            || (entries.is_empty() && (source_bytes != 0 || replacement_bytes != 0))
        {
            return Err(invalid());
        }
        Ok(Self {
            request,
            generation,
            source_bytes,
            replacement_bytes,
            recent_preserved,
            pinned_preserved,
            unresolved_preserved,
            unsavable_preserved,
            entries,
        })
    }
    /// Borrows the exact proposal request.
    #[must_use]
    pub const fn request(&self) -> &WorkbenchCompactionRequest {
        &self.request
    }
    /// Returns the proposed monotonic prompt-view generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    /// Returns total original public-reply bytes replaced by handles.
    #[must_use]
    pub const fn source_bytes(&self) -> u64 {
        self.source_bytes
    }
    /// Returns total deterministic replacement bytes.
    #[must_use]
    pub const fn replacement_bytes(&self) -> u64 {
        self.replacement_bytes
    }
    /// Borrows source-bound replacement metadata.
    #[must_use]
    pub fn entries(&self) -> &[WorkbenchCompactionEntry] {
        &self.entries
    }
    /// Returns whether this preview has a validated reduction that can be applied.
    #[must_use]
    pub const fn applicable(&self) -> bool {
        !self.entries.is_empty()
    }
    /// Returns complete recent replies preserved verbatim.
    #[must_use]
    pub const fn recent_preserved(&self) -> u32 {
        self.recent_preserved
    }
    /// Returns explicitly pinned replies preserved verbatim.
    #[must_use]
    pub const fn pinned_preserved(&self) -> u32 {
        self.pinned_preserved
    }
    /// Returns replies preserved because typed run state could not prove resolved success.
    #[must_use]
    pub const fn unresolved_preserved(&self) -> u32 {
        self.unresolved_preserved
    }
    /// Returns otherwise eligible sources preserved because a handle would not save bytes.
    #[must_use]
    pub const fn unsavable_preserved(&self) -> u32 {
        self.unsavable_preserved
    }
}

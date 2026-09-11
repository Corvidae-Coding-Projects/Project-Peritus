//! Read-only bounded context metadata; never carries private model content or credentials.

use crate::{
    AppErrorCode, AppProtocolError, WorkbenchInputSelection, WorkbenchInvocationId, WorkbenchQuery,
};
use peritus_types::Sha256Digest;

#[cfg(test)]
mod tests;

/// Maximum metadata rows in a response page.
pub const MAX_WORKBENCH_CONTEXT_PAGE: usize = 32;
/// Maximum combined message and source rows in one inspected view.
pub const MAX_WORKBENCH_CONTEXT_ROWS: usize = 8192;

const fn invalid() -> AppProtocolError {
    AppProtocolError::new(AppErrorCode::MalformedFrame, None)
}

/// Distinguishes an eligible input view from immutable request history.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchContextView {
    /// Current eligible inputs, not a claim that a complete request has been constructed.
    Next,
    /// Immutable invocation index, ordered by incorporation.
    History,
    /// Exact source manifest used to construct the selected request.
    Invocation(WorkbenchInvocationId),
}

/// Revision-fenced read-only context query.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchContextQuery {
    query: WorkbenchQuery,
    revision: u64,
    offset: u32,
    view: WorkbenchContextView,
}
impl WorkbenchContextQuery {
    /// Validates the first-page/current or subsequent-page/exact-revision fence.
    ///
    /// # Errors
    /// Rejects unfenced subsequent pages or offsets beyond the bounded view.
    pub const fn new(
        query: WorkbenchQuery,
        revision: u64,
        offset: u32,
        view: WorkbenchContextView,
    ) -> Result<Self, AppProtocolError> {
        if (offset != 0 && revision == 0) || offset as usize > MAX_WORKBENCH_CONTEXT_ROWS {
            return Err(invalid());
        }
        Ok(Self { query, revision, offset, view })
    }
    /// Returns exact conversation and workspace scope.
    #[must_use]
    pub const fn query(self) -> WorkbenchQuery {
        self.query
    }
    /// Returns the aggregate revision fence, or zero for the current first page.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
    /// Returns the first requested metadata row.
    #[must_use]
    pub const fn offset(self) -> u32 {
        self.offset
    }
    /// Returns the explicitly selected view.
    #[must_use]
    pub const fn view(self) -> WorkbenchContextView {
        self.view
    }
}

/// Provider-neutral role of a complete sealed message, not a reasoning transcript.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchMessageRole {
    /// Host system instructions.
    System,
    /// Developer instructions.
    Developer,
    /// User input.
    User,
    /// Prior assistant output or private replay state; content is not exposed.
    Assistant,
    /// Complete tool result message.
    Tool,
}

/// Source identity; categories are explicit and cannot be inferred from displayed text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchContextSource {
    /// Exact retained file reference and version selected for this context view.
    File {
        /// Original source reference operation.
        attachment: crate::ControlOperationId,
        /// Exact immutable version operation.
        version: crate::ControlOperationId,
    },
    /// Explicit immutable image; digest covers original encoded bytes, not its caption.
    Image {
        /// Original import operation, scoped by the query's conversation/workspace.
        operation: crate::ControlOperationId,
        /// Stable caption/queue input identity.
        input: crate::WorkbenchInputId,
        /// Original uploaded artifact identity, not a filesystem grant.
        artifact: peritus_types::ArtifactId,
    },
    /// Exact accepted public input revision; digest covers its UTF-8 bytes.
    Input(WorkbenchInputSelection),
    /// Host-delivered public reply; digest covers its UTF-8 bytes.
    PublicReply(WorkbenchInvocationId),
    /// Complete ordered request message; digest covers C5 single-message archive bytes.
    Message {
        /// Zero-based message position in the sealed request.
        ordinal: u32,
        /// Sealed role, without message content.
        role: WorkbenchMessageRole,
    },
    /// History row whose digest covers canonical request bytes.
    Invocation {
        /// Selected immutable invocation.
        id: WorkbenchInvocationId,
        /// Independently sealed source manifest digest.
        manifest_digest: Sha256Digest,
    },
}

/// Observed inclusion and selection reason. None implies provider delivery or token billing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchContextDisposition {
    /// Eligible durable input, not yet a sealed complete request.
    Eligible,
    /// Included in the exact selected sealed request.
    Included,
    /// Explicitly held by the user.
    Held,
    /// Explicitly withdrawn before incorporation.
    Withdrawn,
    /// Replaced by a newer unincorporated revision.
    Superseded,
    /// Excluded because a prerequisite is held or otherwise unavailable.
    DependencyBlocked,
    /// Public reply retained until a later user turn needs it.
    AwaitingLaterInput,
    /// Explicit image selection was turned off; immutable history remains retained.
    Deselected,
    /// Explicitly excluded by a user context preference; immutable history remains retained.
    UserExcluded,
}

/// Explicit user treatment of one optional next-request source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkbenchContextPreference {
    /// Retain the selected source under the next-request context policy.
    Pinned,
    /// Exclude an optional source without deleting its history.
    Excluded,
}

/// Content-free digest and size for one exact source or request message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchContextRow {
    source: WorkbenchContextSource,
    digest: Sha256Digest,
    bytes: u64,
    disposition: WorkbenchContextDisposition,
    preference: Option<WorkbenchContextPreference>,
}
impl WorkbenchContextRow {
    /// Validates a bounded nonempty source. Size is exact encoded bytes, never tokens.
    ///
    /// # Errors
    /// Rejects zero/excessive sizes, invalid message positions, or impossible sealed states.
    pub const fn new(
        source: WorkbenchContextSource,
        digest: Sha256Digest,
        bytes: u64,
        disposition: WorkbenchContextDisposition,
    ) -> Result<Self, AppProtocolError> {
        if (bytes == 0 && !matches!(source, WorkbenchContextSource::File { .. }))
            || bytes > 64 * 1024 * 1024
        {
            return Err(invalid());
        }
        match source {
            WorkbenchContextSource::Message { ordinal, .. } if ordinal >= 4096 => {
                return Err(invalid());
            }
            WorkbenchContextSource::Message { .. } | WorkbenchContextSource::Invocation { .. }
                if !matches!(disposition, WorkbenchContextDisposition::Included) =>
            {
                return Err(invalid());
            }
            _ => {}
        }
        Ok(Self { source, digest, bytes, disposition, preference: None })
    }
    /// Adds an explicit user preference to a validated next-request row.
    ///
    /// # Errors
    /// Rejects immutable sealed rows and exclusion of mandatory user inputs.
    pub const fn with_preference(
        mut self,
        preference: WorkbenchContextPreference,
    ) -> Result<Self, AppProtocolError> {
        if matches!(
            self.source,
            WorkbenchContextSource::Message { .. } | WorkbenchContextSource::Invocation { .. }
        ) || (matches!(self.source, WorkbenchContextSource::Input(_))
            && matches!(preference, WorkbenchContextPreference::Excluded))
            || (matches!(preference, WorkbenchContextPreference::Excluded)
                != matches!(self.disposition, WorkbenchContextDisposition::UserExcluded))
        {
            return Err(invalid());
        }
        self.preference = Some(preference);
        Ok(self)
    }
    /// Returns exact source identity and category.
    #[must_use]
    pub const fn source(&self) -> WorkbenchContextSource {
        self.source
    }
    /// Returns exact digest under the source category's documented encoding.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
    /// Returns byte count under the same encoding as the digest.
    #[must_use]
    pub const fn bytes(&self) -> u64 {
        self.bytes
    }
    /// Returns actual inclusion or the explicit eligibility/exclusion reason.
    #[must_use]
    pub const fn disposition(&self) -> WorkbenchContextDisposition {
        self.disposition
    }
    /// Returns the explicit user preference, separate from observed lifecycle.
    #[must_use]
    pub const fn preference(&self) -> Option<WorkbenchContextPreference> {
        self.preference
    }
}

/// Exact immutable request/manifest binding selected for inspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WorkbenchContextSeal {
    invocation: WorkbenchInvocationId,
    request_digest: Sha256Digest,
    manifest_digest: Sha256Digest,
    generation: u64,
}
impl WorkbenchContextSeal {
    /// Creates a content-free projection of the already validated durable binding.
    #[must_use]
    pub const fn new(
        invocation: WorkbenchInvocationId,
        request_digest: Sha256Digest,
        manifest_digest: Sha256Digest,
        generation: u64,
    ) -> Self {
        Self { invocation, request_digest, manifest_digest, generation }
    }
    /// Returns selected request invocation.
    #[must_use]
    pub const fn invocation(self) -> WorkbenchInvocationId {
        self.invocation
    }
    /// Returns canonical semantic request digest.
    #[must_use]
    pub const fn request_digest(self) -> Sha256Digest {
        self.request_digest
    }
    /// Returns exact immutable manifest digest.
    #[must_use]
    pub const fn manifest_digest(self) -> Sha256Digest {
        self.manifest_digest
    }
    /// Returns the governing input generation actually used.
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }
}

/// Exact bounded context page; no omission is presented as a complete view.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkbenchContextPage {
    query: WorkbenchContextQuery,
    total: u32,
    seal: Option<WorkbenchContextSeal>,
    rows: Vec<WorkbenchContextRow>,
}
impl WorkbenchContextPage {
    /// Validates complete page coverage and that any seal matches the selected invocation.
    ///
    /// # Errors
    /// Rejects absent revision, gaps, bounds, duplicate sources, or a mismatched view/seal.
    pub fn new(
        query: WorkbenchContextQuery,
        total: u32,
        seal: Option<WorkbenchContextSeal>,
        rows: Vec<WorkbenchContextRow>,
    ) -> Result<Self, AppProtocolError> {
        if query.revision() == 0
            || query.offset() > total
            || total as usize > MAX_WORKBENCH_CONTEXT_ROWS
            || rows.len()
                != (total.saturating_sub(query.offset()) as usize).min(MAX_WORKBENCH_CONTEXT_PAGE)
            || rows
                .iter()
                .enumerate()
                .any(|(i, row)| rows[..i].iter().any(|prior| prior.source() == row.source()))
        {
            return Err(invalid());
        }
        match (query.view(), seal) {
            (WorkbenchContextView::Invocation(id), Some(seal)) if id == seal.invocation() => {}
            (WorkbenchContextView::Next | WorkbenchContextView::History, None) => {}
            _ => return Err(invalid()),
        }
        if rows.iter().any(|row| match query.view() {
            WorkbenchContextView::Next => {
                !matches!(
                    row.source(),
                    WorkbenchContextSource::Input(_)
                        | WorkbenchContextSource::PublicReply(_)
                        | WorkbenchContextSource::Image { .. }
                        | WorkbenchContextSource::File { .. }
                ) || matches!(row.disposition(), WorkbenchContextDisposition::Included)
            }
            WorkbenchContextView::History => {
                !matches!(row.source(), WorkbenchContextSource::Invocation { .. })
            }
            WorkbenchContextView::Invocation(_) => {
                matches!(row.source(), WorkbenchContextSource::Invocation { .. })
                    || !matches!(row.disposition(), WorkbenchContextDisposition::Included)
            }
        }) {
            return Err(invalid());
        }
        Ok(Self { query, total, seal, rows })
    }
    /// Returns exact scope, revision, view, and page parameters.
    #[must_use]
    pub const fn query(&self) -> WorkbenchContextQuery {
        self.query
    }
    /// Returns total row count in the complete inspected view.
    #[must_use]
    pub const fn total(&self) -> u32 {
        self.total
    }
    /// Returns immutable invocation binding, absent for eligible-input/index views.
    #[must_use]
    pub const fn seal(&self) -> Option<WorkbenchContextSeal> {
        self.seal
    }
    /// Borrows this exact metadata page.
    #[must_use]
    pub fn rows(&self) -> &[WorkbenchContextRow] {
        &self.rows
    }
}

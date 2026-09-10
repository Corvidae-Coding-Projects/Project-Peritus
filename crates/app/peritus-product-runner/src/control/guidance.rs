//! Typed inert audit payloads for daemon-owned project-guidance sidecars.

use super::{ControlError, ControlText, ConversationId, InvocationId, OperationId};
use serde::Deserialize;
use serde::Serialize;

/// Exact active-record selection independently fenced from the conversation revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuidanceSelection {
    id: OperationId,
    expected_revision: u64,
}

impl GuidanceSelection {
    /// Constructs an exact selected record revision.
    ///
    /// # Errors
    /// Rejects revision zero.
    pub const fn new(id: OperationId, expected_revision: u64) -> Result<Self, ControlError> {
        if expected_revision == 0 {
            return Err(ControlError::InvalidInput);
        }
        Ok(Self { id, expected_revision })
    }

    /// Returns the stable initial-save operation identity.
    #[must_use]
    pub const fn id(self) -> OperationId {
        self.id
    }

    /// Returns the exact selected record revision.
    #[must_use]
    pub const fn expected_revision(self) -> u64 {
        self.expected_revision
    }
}

/// Project-first eligibility scope; no cross-workspace scope is representable.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum GuidanceScope {
    /// Every conversation in the operation's exact workspace.
    Project,
    /// Only one selected conversation in that workspace.
    Conversation(ConversationId),
}

/// Provenance of text the user explicitly approved for reuse.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum GuidanceSource {
    /// Direct user-authored text.
    UserAuthored,
    /// Exact immutable public reply separately accepted by the user.
    AcceptedPublicReply {
        /// Reply publication operation.
        operation: OperationId,
        /// Invocation immediately preceding the reply.
        invocation: InvocationId,
        /// SHA-256 of the exact accepted text.
        digest: [u8; 32],
    },
}

/// Exact guidance text, original provenance, and within-project scope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuidanceContent {
    text: ControlText<8192>,
    source: GuidanceSource,
    scope: GuidanceScope,
}

impl GuidanceContent {
    /// Binds validated text to its exact source and scope.
    ///
    /// # Errors
    /// Rejects accepted-reply provenance whose digest differs from the text.
    pub fn new(
        text: ControlText<8192>,
        source: GuidanceSource,
        scope: GuidanceScope,
    ) -> Result<Self, ControlError> {
        if matches!(source, GuidanceSource::AcceptedPublicReply { digest, .. } if digest != peritus_codec::sha256(text.as_str().as_bytes()).into_bytes())
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(Self { text, source, scope })
    }

    /// Borrows exact user-approved text.
    #[must_use]
    pub const fn text(&self) -> &ControlText<8192> {
        &self.text
    }

    /// Returns exact source provenance.
    #[must_use]
    pub const fn source(&self) -> GuidanceSource {
        self.source
    }

    /// Returns project or selected-conversation scope.
    #[must_use]
    pub const fn scope(&self) -> GuidanceScope {
        self.scope
    }
}

/// Exact bounded guidance mutation retained in the immutable conversation audit event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum GuidanceAudit {
    /// Saves a new stable guidance identity bound to the enclosing operation.
    Save {
        /// Workspace guidance dependency revision inspected by the user.
        expected_dependency_revision: u64,
        /// Exact approved content.
        content: GuidanceContent,
        /// Initial retrieval-priority preference.
        pinned: bool,
    },
    /// Replaces the content of one exact active record.
    Revise {
        /// Exact selected identity and record revision.
        selection: GuidanceSelection,
        /// Workspace guidance dependency revision inspected by the user.
        expected_dependency_revision: u64,
        /// Exact replacement content.
        content: GuidanceContent,
    },
    /// Changes retrieval priority without revalidating unchanged text.
    Pin {
        /// Exact selected identity and record revision.
        selection: GuidanceSelection,
        /// Workspace guidance dependency revision inspected by the user.
        expected_dependency_revision: u64,
        /// Desired pin state.
        pinned: bool,
    },
    /// Changes eligibility only within the already-bound project.
    Scope {
        /// Exact selected identity and record revision.
        selection: GuidanceSelection,
        /// Workspace guidance dependency revision inspected by the user.
        expected_dependency_revision: u64,
        /// New project or selected-conversation scope.
        scope: GuidanceScope,
    },
    /// Writes a content-free tombstone while the earlier audit event remains immutable.
    Forget {
        /// Exact selected identity and record revision.
        selection: GuidanceSelection,
        /// Workspace guidance dependency revision inspected by the user.
        expected_dependency_revision: u64,
        /// Exact user-supplied explanation of the exclusion.
        reason: ControlText<1024>,
    },
}

impl GuidanceAudit {
    /// Constructs an initial save audit; dependency zero is valid for an empty workspace catalog.
    #[must_use]
    pub const fn save(
        expected_dependency_revision: u64,
        content: GuidanceContent,
        pinned: bool,
    ) -> Self {
        Self::Save { expected_dependency_revision, content, pinned }
    }

    /// Constructs an exact replacement-content audit.
    ///
    /// # Errors
    /// Rejects dependency revision zero for an existing selected record.
    pub fn revise(
        selection: GuidanceSelection,
        expected_dependency_revision: u64,
        content: GuidanceContent,
    ) -> Result<Self, ControlError> {
        if expected_dependency_revision == 0 {
            return Err(ControlError::InvalidInput);
        }
        Ok(Self::Revise { selection, expected_dependency_revision, content })
    }

    /// Constructs an exact pin-state audit.
    ///
    /// # Errors
    /// Rejects dependency revision zero for an existing selected record.
    pub const fn pin(
        selection: GuidanceSelection,
        expected_dependency_revision: u64,
        pinned: bool,
    ) -> Result<Self, ControlError> {
        if expected_dependency_revision == 0 {
            return Err(ControlError::InvalidInput);
        }
        Ok(Self::Pin { selection, expected_dependency_revision, pinned })
    }

    /// Constructs an exact within-project scope audit.
    ///
    /// # Errors
    /// Rejects dependency revision zero for an existing selected record.
    pub const fn scope(
        selection: GuidanceSelection,
        expected_dependency_revision: u64,
        scope: GuidanceScope,
    ) -> Result<Self, ControlError> {
        if expected_dependency_revision == 0 {
            return Err(ControlError::InvalidInput);
        }
        Ok(Self::Scope { selection, expected_dependency_revision, scope })
    }

    /// Constructs an exact forget audit.
    ///
    /// # Errors
    /// Rejects dependency revision zero for an existing selected record.
    pub fn forget(
        selection: GuidanceSelection,
        expected_dependency_revision: u64,
        reason: ControlText<1024>,
    ) -> Result<Self, ControlError> {
        if expected_dependency_revision == 0 {
            return Err(ControlError::InvalidInput);
        }
        Ok(Self::Forget { selection, expected_dependency_revision, reason })
    }

    /// Returns the exact workspace dependency revision the user inspected.
    #[must_use]
    pub const fn expected_dependency_revision(&self) -> u64 {
        match self {
            Self::Save { expected_dependency_revision, .. }
            | Self::Revise { expected_dependency_revision, .. }
            | Self::Pin { expected_dependency_revision, .. }
            | Self::Scope { expected_dependency_revision, .. }
            | Self::Forget { expected_dependency_revision, .. } => *expected_dependency_revision,
        }
    }
}

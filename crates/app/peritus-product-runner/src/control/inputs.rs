//! Immutable input revisions and dependency-safe queue transitions, independent of transport.

mod capture;
#[cfg(test)]
mod tests;
mod transition;

pub use capture::InputCapture;

use super::{ControlError, ControlText, InputId, InvocationId};
use serde::Deserialize;
use serde::Serialize;

pub(super) const SOURCE_BACKED_INPUT_NOTICE: &str =
    "[source-backed user input; read the authoritative request source]";

/// Lifecycle of one immutable content revision.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputState {
    /// Eligible at a subsequent explicitly bound provider boundary.
    Queued,
    /// Retained but excluded from incorporation.
    Held,
    /// Exact revision was included in one durable request binding.
    Incorporated,
    /// A later revision of this item replaced it before incorporation.
    Superseded,
    /// Explicitly removed before incorporation; preserved in history.
    Withdrawn,
}

/// One exact ID/revision pair, not a moving reference to latest text.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputSelection {
    id: InputId,
    revision: u64,
}
impl InputSelection {
    /// Validates a positive immutable content revision.
    ///
    /// # Errors
    /// Rejects revision zero.
    pub const fn new(id: InputId, revision: u64) -> Result<Self, ControlError> {
        if revision == 0 { Err(ControlError::InvalidInput) } else { Ok(Self { id, revision }) }
    }
    /// Returns stable input identity.
    #[must_use]
    pub const fn id(self) -> InputId {
        self.id
    }
    /// Returns exact content revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
}

/// Immutable body handle for one complete source-backed user input.
///
/// The referenced artifact is published and verified before the queue operation can be accepted.
/// It is data, not a separately executable input or a mutable upload session.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RequestSource {
    artifact: [u8; 16],
    digest: [u8; 32],
    bytes: u64,
}
impl RequestSource {
    /// Creates one exact completed-source binding.
    ///
    /// # Errors
    /// Rejects the all-zero artifact identity or an empty body.
    pub fn new(
        artifact: [u8; 16],
        digest: [u8; 32],
        bytes: u64,
    ) -> Result<Self, ControlError> {
        if artifact == [0; 16] || bytes == 0 {
            Err(ControlError::InvalidInput)
        } else {
            Ok(Self { artifact, digest, bytes })
        }
    }
    /// Returns the immutable application-artifact identity.
    #[must_use]
    pub const fn artifact_bytes(self) -> [u8; 16] {
        self.artifact
    }
    /// Returns the full-body SHA-256 digest.
    #[must_use]
    pub const fn digest(self) -> [u8; 32] {
        self.digest
    }
    /// Returns the exact full-body byte length.
    #[must_use]
    pub const fn bytes(self) -> u64 {
        self.bytes
    }
}

/// An accepted input revision; edits never mutate its content or author.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputRevision {
    selection: InputSelection,
    author: [u8; 16],
    text: ControlText<8192>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source: Option<RequestSource>,
    dependencies: Vec<InputId>,
    state: InputState,
    correction_of: Option<InputSelection>,
    invocation: Option<InvocationId>,
}
impl InputRevision {
    /// Returns exact input identity and revision.
    #[must_use]
    pub const fn selection(&self) -> InputSelection {
        self.selection
    }
    /// Borrows exact public text; this never contains private reasoning.
    #[must_use]
    pub fn text(&self) -> &str {
        self.text.as_str()
    }
    /// Returns the complete immutable source handle when the body is retained out of line.
    #[must_use]
    pub const fn source(&self) -> Option<RequestSource> {
        self.source
    }
    /// Returns current lifecycle of this immutable content revision.
    #[must_use]
    pub const fn state(&self) -> InputState {
        self.state
    }
    /// Borrows recorded author identity from authenticated admission.
    #[must_use]
    pub const fn author_bytes(&self) -> &[u8; 16] {
        &self.author
    }
    /// Borrows the item's declared prerequisites.
    #[must_use]
    pub fn dependencies(&self) -> &[InputId] {
        &self.dependencies
    }
    /// Returns the immutable older input explicitly corrected by this new item.
    #[must_use]
    pub const fn correction_of(&self) -> Option<InputSelection> {
        self.correction_of
    }
    /// Returns the exact invocation that incorporated this revision.
    #[must_use]
    pub const fn invocation(&self) -> Option<InvocationId> {
        self.invocation
    }
}

/// Request capture published atomically with every selected item's incorporation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvocationInputs {
    invocation: InvocationId,
    request_digest: [u8; 32],
    manifest_digest: [u8; 32],
    items: Vec<InputSelection>,
}
impl InvocationInputs {
    /// Returns request invocation identity.
    #[must_use]
    pub const fn invocation(&self) -> InvocationId {
        self.invocation
    }
    /// Returns the exact canonical semantic request digest supplied by the host.
    #[must_use]
    pub const fn request_digest(&self) -> peritus_types::Sha256Digest {
        peritus_types::Sha256Digest::new(self.request_digest)
    }
    /// Returns the immutable source manifest bound by the same incorporation event.
    #[must_use]
    pub const fn manifest_digest(&self) -> peritus_types::Sha256Digest {
        peritus_types::Sha256Digest::new(self.manifest_digest)
    }
    /// Borrows the exact immutable selections in provider request order.
    #[must_use]
    pub fn items(&self) -> &[InputSelection] {
        &self.items
    }
}

/// Typed queue transitions. Incorporation is host-only and must not be exposed as a user grant.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(deny_unknown_fields)]
pub enum QueueIntent {
    /// Accepts a new user input; dependencies must already exist in this ledger.
    Enqueue {
        /// Stable new identity.
        id: InputId,
        /// Exact bounded content.
        text: ControlText<8192>,
        /// Required earlier inputs.
        dependencies: Vec<InputId>,
    },
    /// Atomically accepts one complete user input whose exact body is a completed artifact.
    EnqueueSource {
        /// Stable new identity.
        id: InputId,
        /// Complete immutable body handle.
        source: RequestSource,
        /// Required earlier inputs.
        dependencies: Vec<InputId>,
    },
    /// Supersedes only a queued/held exact revision, retaining the old content and state.
    Edit {
        /// Exact revision the user inspected.
        selected: InputSelection,
        /// Replacement content.
        text: ControlText<8192>,
    },
    /// Atomically supersedes one pending revision with a complete source-backed body.
    EditSource {
        /// Exact revision the user inspected.
        selected: InputSelection,
        /// Complete immutable replacement body.
        source: RequestSource,
    },
    /// Creates a new correction after an incorporated immutable input.
    Correct {
        /// Exact incorporated revision being corrected.
        original: InputSelection,
        /// New correction identity.
        id: InputId,
        /// Corrective content.
        text: ControlText<8192>,
    },
    /// Creates a complete source-backed correction after an incorporated immutable input.
    CorrectSource {
        /// Exact incorporated revision being corrected.
        original: InputSelection,
        /// New correction identity.
        id: InputId,
        /// Complete immutable corrective body.
        source: RequestSource,
    },
    /// Changes only held/queued status of an unincorporated exact revision.
    Hold {
        /// Exact revision selected.
        selected: InputSelection,
        /// True holds, false releases.
        held: bool,
    },
    /// Withdraws an unincorporated item; dependent pending items must be handled first.
    Withdraw(InputSelection),
    /// Moves one exact pending revision to a zero-based position in the resulting order.
    Move {
        /// Exact pending revision being moved.
        selected: InputSelection,
        /// Zero-based position after removing the selected item from its current position.
        position: u64,
    },
    /// Reorders the complete current pending identity set without violating dependencies.
    Reorder(Vec<InputId>),
    /// Host transaction captures exact inputs and marks incorporation together.
    Incorporate {
        /// Unique request invocation identity.
        invocation: InvocationId,
        /// Exact canonical semantic request digest, not a provider-completion receipt.
        request_digest: [u8; 32],
        /// Exact immutable source manifest digest, independently bound to this event.
        manifest_digest: [u8; 32],
        /// Exact selected revisions in current order.
        items: Vec<InputSelection>,
    },
}

/// Complete input history plus pending order and immutable request bindings.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputLedger {
    revisions: Vec<InputRevision>,
    order: Vec<InputId>,
    invocations: Vec<InvocationInputs>,
    generation: u64,
}
impl InputLedger {
    /// Returns whether no input transition has ever been accepted.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.revisions.is_empty()
            && self.order.is_empty()
            && self.invocations.is_empty()
            && self.generation == 0
    }
    /// Returns the generation used to fence stale pending effects after accepted input changes.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }
    /// Borrows complete bounded immutable content history, not just the recent activity projection.
    #[must_use]
    pub fn revisions(&self) -> &[InputRevision] {
        &self.revisions
    }
    /// Borrows current pending order; held items remain visible in their exact position.
    #[must_use]
    pub fn order(&self) -> &[InputId] {
        &self.order
    }
    /// Borrows immutable request incorporation bindings.
    #[must_use]
    pub fn invocations(&self) -> &[InvocationInputs] {
        &self.invocations
    }
    /// Returns the latest accepted revision of an input without changing historical content.
    #[must_use]
    pub fn latest(&self, id: InputId) -> Option<&InputRevision> {
        self.revisions.iter().rev().find(|item| item.selection.id == id)
    }
}

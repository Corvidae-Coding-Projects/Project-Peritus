//! Ordered bounded progress envelope.

use crate::{BoundedJson, BoundedText, PreparedToolCall, ProtocolError, ProtocolErrorKind};
use peritus_policy::AuthorityInstant;
use peritus_types::{ActionId, Sha256Digest};

/// Closed progress classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProgressKind {
    /// The implementation accepted the invocation.
    Started,
    /// Structured output or state advanced.
    Update,
    /// A control was applied.
    Control,
    /// Cancellation or deadline handling began.
    Stopping,
    /// Recovery observation was performed.
    Recovery,
}

/// One invocation-bound progress event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ToolProgress {
    action_id: ActionId,
    prepared_digest: Sha256Digest,
    sequence: u32,
    kind: ProgressKind,
    observed_at: AuthorityInstant,
    structured: Option<BoundedJson>,
    model_rendering: BoundedText,
}

impl ToolProgress {
    /// Creates one bounded progress event.
    ///
    /// # Errors
    ///
    /// Rejects sequences outside the call's progress ceiling.
    pub fn new(
        prepared: &PreparedToolCall,
        sequence: u32,
        kind: ProgressKind,
        observed_at: AuthorityInstant,
        structured: Option<BoundedJson>,
        model_rendering: BoundedText,
    ) -> Result<Self, ProtocolError> {
        if sequence >= prepared.call().limits().progress_events()
            || model_rendering.as_str().len() > prepared.call().limits().model_bytes() as usize
            || observed_at.epoch() != prepared.call().deadline().epoch()
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidEnvelope,
                "progress",
                "progress sequence, rendering, or authority epoch exceeds the call envelope",
            ));
        }
        Ok(Self {
            action_id: prepared.call().action_id(),
            prepared_digest: prepared.prepared_digest(),
            sequence,
            kind,
            observed_at,
            structured,
            model_rendering,
        })
    }

    /// Returns the producing action.
    #[must_use]
    pub const fn action_id(&self) -> ActionId {
        self.action_id
    }
    /// Returns the prepared-call digest.
    #[must_use]
    pub const fn prepared_digest(&self) -> Sha256Digest {
        self.prepared_digest
    }
    /// Returns the zero-based sequence.
    #[must_use]
    pub const fn sequence(&self) -> u32 {
        self.sequence
    }
    /// Returns the closed progress kind.
    #[must_use]
    pub const fn kind(&self) -> ProgressKind {
        self.kind
    }
    /// Returns the observation instant.
    #[must_use]
    pub const fn observed_at(&self) -> AuthorityInstant {
        self.observed_at
    }
    /// Borrows optional structured progress.
    #[must_use]
    pub const fn structured(&self) -> Option<&BoundedJson> {
        self.structured.as_ref()
    }
    /// Borrows the bounded model rendering.
    #[must_use]
    pub const fn model_rendering(&self) -> &BoundedText {
        &self.model_rendering
    }

    /// Returns stable version-one canonical progress-envelope bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut bytes = crate::wire::begin(4);
        bytes.extend_from_slice(self.action_id.as_bytes());
        bytes.extend_from_slice(self.prepared_digest.as_bytes());
        crate::wire::u32_value(&mut bytes, self.sequence);
        bytes.push(match self.kind {
            ProgressKind::Started => 1,
            ProgressKind::Update => 2,
            ProgressKind::Control => 3,
            ProgressKind::Stopping => 4,
            ProgressKind::Recovery => 5,
        });
        crate::wire::instant(&mut bytes, self.observed_at);
        match &self.structured {
            Some(value) => {
                bytes.push(1);
                crate::wire::bytes(&mut bytes, value.canonical_bytes());
            }
            None => bytes.push(0),
        }
        crate::wire::text(&mut bytes, self.model_rendering.as_str());
        bytes
    }
}

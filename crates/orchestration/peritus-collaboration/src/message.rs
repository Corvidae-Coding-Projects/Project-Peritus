//! Bounded inert collaboration messages and delivery state.

use peritus_types::{ActorId, RevisionTuple, Sha256Digest};

use crate::error::{CollaborationError, CollaborationErrorKind, reject};
use crate::{ArtifactHandoff, CollaborationMessageId, CollaborationTaskId};

/// One immutable causal message. Content is addressed by digest and never executable authority.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollaborationMessage {
    id: CollaborationMessageId,
    root_task_id: CollaborationTaskId,
    task_id: CollaborationTaskId,
    sender: ActorId,
    receiver: ActorId,
    ordinal: u32,
    predecessor: Option<CollaborationMessageId>,
    media_type: String,
    payload_bytes: u32,
    content_digest: Sha256Digest,
    artifact: Option<ArtifactHandoff>,
    revision: RevisionTuple,
}

impl CollaborationMessage {
    /// Creates a syntax-checked inert message.
    ///
    /// State-dependent predecessor, ordinal, owner, and bound checks happen in the reducer.
    ///
    /// # Errors
    /// Rejects zero ordinal/payload, malformed media type, and zero content digest.
    #[allow(clippy::too_many_arguments, reason = "causal message fields stay explicit")]
    pub fn new(
        id: CollaborationMessageId,
        root_task_id: CollaborationTaskId,
        task_id: CollaborationTaskId,
        sender: ActorId,
        receiver: ActorId,
        ordinal: u32,
        predecessor: Option<CollaborationMessageId>,
        media_type: impl Into<String>,
        payload_bytes: u32,
        content_digest: Sha256Digest,
        artifact: Option<ArtifactHandoff>,
        revision: RevisionTuple,
    ) -> Result<Self, CollaborationError> {
        let media_type = media_type.into();
        if ordinal == 0
            || payload_bytes == 0
            || media_type.is_empty()
            || media_type.len() > 255
            || !valid_media_type(&media_type)
            || content_digest == Sha256Digest::new([0; 32])
        {
            return Err(reject(
                CollaborationErrorKind::InvalidInput,
                "message ordinal, payload, media type, or digest is invalid",
            ));
        }
        if (ordinal == 1) != predecessor.is_none() {
            return Err(reject(
                CollaborationErrorKind::CausalityViolation,
                "message predecessor shape differs from its ordinal",
            ));
        }
        Ok(Self {
            id,
            root_task_id,
            task_id,
            sender,
            receiver,
            ordinal,
            predecessor,
            media_type,
            payload_bytes,
            content_digest,
            artifact,
            revision,
        })
    }

    /// Returns the message identity.
    #[must_use]
    pub const fn id(&self) -> CollaborationMessageId {
        self.id
    }
    /// Returns the root task identity.
    #[must_use]
    pub const fn root_task_id(&self) -> CollaborationTaskId {
        self.root_task_id
    }
    /// Returns the causal task identity.
    #[must_use]
    pub const fn task_id(&self) -> CollaborationTaskId {
        self.task_id
    }
    /// Returns the sending actor.
    #[must_use]
    pub const fn sender(&self) -> ActorId {
        self.sender
    }
    /// Returns the receiving actor.
    #[must_use]
    pub const fn receiver(&self) -> ActorId {
        self.receiver
    }
    /// Returns the contiguous per-task message ordinal.
    #[must_use]
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }
    /// Returns the preceding message in this task.
    #[must_use]
    pub const fn predecessor(&self) -> Option<CollaborationMessageId> {
        self.predecessor
    }
    /// Borrows the checked media type.
    #[must_use]
    pub fn media_type(&self) -> &str {
        &self.media_type
    }
    /// Returns the bounded payload length.
    #[must_use]
    pub const fn payload_bytes(&self) -> u32 {
        self.payload_bytes
    }
    /// Returns the content-addressed payload digest.
    #[must_use]
    pub const fn content_digest(&self) -> Sha256Digest {
        self.content_digest
    }
    /// Returns the exact artifact handoff reference when present.
    #[must_use]
    pub const fn artifact(&self) -> Option<ArtifactHandoff> {
        self.artifact
    }
    /// Returns the exact revision.
    #[must_use]
    pub const fn revision(&self) -> RevisionTuple {
        self.revision
    }
}

/// Durable delivery acknowledgement paired with its immutable message.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageDelivery {
    message: CollaborationMessage,
    acknowledged: bool,
}

impl MessageDelivery {
    pub(super) const fn pending(message: CollaborationMessage) -> Self {
        Self { message, acknowledged: false }
    }
    pub(super) const fn from_wire(message: CollaborationMessage, acknowledged: bool) -> Self {
        Self { message, acknowledged }
    }
    /// Borrows the immutable message.
    #[must_use]
    pub const fn message(&self) -> &CollaborationMessage {
        &self.message
    }
    /// Returns whether the exact receiver acknowledged delivery.
    #[must_use]
    pub const fn acknowledged(&self) -> bool {
        self.acknowledged
    }
    pub(super) const fn acknowledge(&mut self) {
        self.acknowledged = true;
    }
}

fn valid_media_type(value: &str) -> bool {
    let Some((kind, subtype)) = value.split_once('/') else {
        return false;
    };
    !kind.is_empty()
        && !subtype.is_empty()
        && !subtype.contains('/')
        && value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'/' | b'!' | b'#' | b'$' | b'&' | b'-' | b'.' | b'+' | b'^' | b'_'
                )
        })
}

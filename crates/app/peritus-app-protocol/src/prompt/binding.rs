//! Immutable prompt correlation and bounded choice/constraint definitions.

use crate::{PromptId, RequestId};
use peritus_types::{ActorId, Generation, RevisionTuple, SessionId, Sha256Digest};

use super::{PromptError, PromptErrorKind, error::reject};

/// Closed prompt purpose.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PromptKind {
    /// Requests unprivileged approve/deny/cancel intent.
    Approval,
    /// Requests one bounded user-input value.
    UserInput,
}

/// One canonical selectable option.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PromptChoice {
    id: String,
    label: String,
}

impl PromptChoice {
    /// Creates a nonempty bounded option identity and display label.
    ///
    /// # Errors
    ///
    /// Rejects zero bounds or empty/oversized text.
    pub fn new(
        id: String,
        label: String,
        maximum_id_bytes: usize,
        maximum_label_bytes: usize,
    ) -> Result<Self, PromptError> {
        if maximum_id_bytes == 0 || maximum_label_bytes == 0 {
            return Err(reject(PromptErrorKind::InvalidLimit, "choice text limit is zero"));
        }
        if id.is_empty()
            || id.len() > maximum_id_bytes
            || label.is_empty()
            || label.len() > maximum_label_bytes
        {
            return Err(reject(
                PromptErrorKind::InvalidInput,
                "choice identity or label is empty or exceeds its bound",
            ));
        }
        Ok(Self { id, label })
    }
    /// Borrows the stable option identity.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
    /// Borrows the display label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// Closed input constraint vocabulary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PromptConstraint {
    /// An empty text value is forbidden.
    NonEmpty,
    /// Text input may contain at most the supplied positive UTF-8 byte count.
    MaximumTextBytes(u32),
    /// The response must name one bound choice.
    BoundChoiceOnly,
    /// Sensitive input should be represented by an opaque secret reference.
    SecretReference,
}

/// Complete immutable correlation/freshness identity echoed by responses.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PromptCorrelation {
    originating_request_id: RequestId,
    prompt_id: PromptId,
    session_id: SessionId,
    actor_id: ActorId,
    revision: RevisionTuple,
    freshness_digest: Sha256Digest,
    cancellation_generation: Generation,
}

impl PromptCorrelation {
    /// Creates the complete prompt correlation value.
    #[must_use]
    pub const fn new(
        originating_request_id: RequestId,
        prompt_id: PromptId,
        session_id: SessionId,
        actor_id: ActorId,
        revision: RevisionTuple,
        freshness_digest: Sha256Digest,
        cancellation_generation: Generation,
    ) -> Self {
        Self {
            originating_request_id,
            prompt_id,
            session_id,
            actor_id,
            revision,
            freshness_digest,
            cancellation_generation,
        }
    }
    /// Returns the request that originated the prompt.
    #[must_use]
    pub const fn originating_request_id(self) -> RequestId {
        self.originating_request_id
    }
    /// Returns the prompt identity.
    #[must_use]
    pub const fn prompt_id(self) -> PromptId {
        self.prompt_id
    }
    /// Returns the target session.
    #[must_use]
    pub const fn session_id(self) -> SessionId {
        self.session_id
    }
    /// Returns the claimed target actor.
    #[must_use]
    pub const fn actor_id(self) -> ActorId {
        self.actor_id
    }
    /// Returns the exact bound revision.
    #[must_use]
    pub const fn revision(self) -> RevisionTuple {
        self.revision
    }
    /// Returns the freshness digest.
    #[must_use]
    pub const fn freshness_digest(self) -> Sha256Digest {
        self.freshness_digest
    }
    /// Returns the cancellation generation.
    #[must_use]
    pub const fn cancellation_generation(self) -> Generation {
        self.cancellation_generation
    }
}

/// Checked immutable prompt binding.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PromptBinding {
    kind: PromptKind,
    correlation: PromptCorrelation,
    choices: Vec<PromptChoice>,
    constraints: Vec<PromptConstraint>,
}

impl PromptBinding {
    /// Creates a bounded canonical prompt binding.
    ///
    /// # Errors
    ///
    /// Rejects zero/exceeded limits, noncanonical choice ordering, invalid constraints, or choices
    /// on an approval prompt.
    pub fn new(
        kind: PromptKind,
        correlation: PromptCorrelation,
        choices: Vec<PromptChoice>,
        constraints: Vec<PromptConstraint>,
        maximum_choices: usize,
        maximum_constraints: usize,
    ) -> Result<Self, PromptError> {
        if maximum_choices == 0 || maximum_constraints == 0 {
            return Err(reject(PromptErrorKind::InvalidLimit, "prompt collection limit is zero"));
        }
        if choices.len() > maximum_choices || constraints.len() > maximum_constraints {
            return Err(reject(
                PromptErrorKind::InvalidInput,
                "prompt choice or constraint count exceeds its bound",
            ));
        }
        if choices.windows(2).any(|pair| pair[0].id() >= pair[1].id()) {
            return Err(reject(
                PromptErrorKind::InvalidInput,
                "prompt choices must be strictly ordered by unique identity",
            ));
        }
        if constraints
            .iter()
            .any(|constraint| matches!(constraint, PromptConstraint::MaximumTextBytes(0)))
        {
            return Err(reject(
                PromptErrorKind::InvalidInput,
                "maximum-text constraint must be positive",
            ));
        }
        if constraints.windows(2).any(|pair| pair[0] >= pair[1]) {
            return Err(reject(
                PromptErrorKind::InvalidInput,
                "prompt constraints must be strictly ordered and unique",
            ));
        }
        if kind == PromptKind::Approval && !choices.is_empty() {
            return Err(reject(
                PromptErrorKind::InvalidInput,
                "approval prompts use closed intent and cannot define choices",
            ));
        }
        Ok(Self { kind, correlation, choices, constraints })
    }

    /// Returns the prompt kind.
    #[must_use]
    pub const fn kind(&self) -> PromptKind {
        self.kind
    }
    /// Returns the complete immutable correlation.
    #[must_use]
    pub const fn correlation(&self) -> PromptCorrelation {
        self.correlation
    }
    /// Borrows canonical choices.
    #[must_use]
    pub fn choices(&self) -> &[PromptChoice] {
        &self.choices
    }
    /// Borrows declared constraints.
    #[must_use]
    pub fn constraints(&self) -> &[PromptConstraint] {
        &self.constraints
    }
}

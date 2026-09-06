//! Additive conversation-first execution requests and honest user-input lifecycle observations.

use super::{
    ProductRoleModels, ProductRunMessageError, ProductRunRequest, ProductRunSnapshot, bounded_text,
};

/// Explicit interaction mode; read-only modes are enforced by the execution tool boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductInteractionMode {
    /// General conversation: follow the user's scope, without forced implementation.
    Chat,
    /// Read-only discussion and planning.
    Plan,
    /// Fresh read-only independent review.
    Review,
    /// Explicitly commissioned writer/check/reviewer/fixer delivery pipeline.
    Build,
}
impl ProductInteractionMode {
    /// Append-only mode encoding.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::Chat => 1,
            Self::Plan => 2,
            Self::Review => 3,
            Self::Build => 4,
        }
    }
    /// Decodes a known mode without converting unknown values into execution.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::Chat),
            2 => Some(Self::Plan),
            3 => Some(Self::Review),
            4 => Some(Self::Build),
            _ => None,
        }
    }
    /// Human-facing mode name.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Chat => "Chat",
            Self::Plan => "Plan · read-only",
            Self::Review => "Review · read-only",
            Self::Build => "Build",
        }
    }
}

/// Starts or continues one exact conversation with explicit mode and model selections.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductInteractionRequest {
    request: ProductRunRequest,
    mode: ProductInteractionMode,
    models: ProductRoleModels,
}
impl ProductInteractionRequest {
    /// Uses the inner task as the next message, not as implicit authorization to build.
    #[must_use]
    pub const fn new(
        request: ProductRunRequest,
        mode: ProductInteractionMode,
        models: ProductRoleModels,
    ) -> Self {
        Self { request, mode, models }
    }
    /// Exact conversation/workspace/provider identities and next user message.
    #[must_use]
    pub const fn request(&self) -> &ProductRunRequest {
        &self.request
    }
    /// Selected interaction mode.
    #[must_use]
    pub const fn mode(&self) -> ProductInteractionMode {
        self.mode
    }
    /// Exact model choices for the selected roles.
    #[must_use]
    pub const fn models(&self) -> &ProductRoleModels {
        &self.models
    }
}

/// Maximum retained public activity entries per conversation observation.
pub const MAX_PRODUCT_ACTIVITIES: usize = 256;
/// Maximum bytes in one expandable public activity detail.
pub const MAX_PRODUCT_ACTIVITY_BYTES: usize = 8192;

/// Kind of public conversation activity; excludes private reasoning and raw provider envelopes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProductActivityKind {
    /// Durable user input.
    User,
    /// Public assistant text.
    Assistant,
    /// Running or completed tool summary.
    Tool,
    /// Input incorporation or execution status.
    Status,
    /// Actionable execution failure.
    Error,
}
impl ProductActivityKind {
    /// Stable wire tag.
    #[must_use]
    pub const fn tag(self) -> u16 {
        match self {
            Self::User => 1,
            Self::Assistant => 2,
            Self::Tool => 3,
            Self::Status => 4,
            Self::Error => 5,
        }
    }
    /// Decodes a known kind.
    #[must_use]
    pub const fn from_tag(tag: u16) -> Option<Self> {
        match tag {
            1 => Some(Self::User),
            2 => Some(Self::Assistant),
            3 => Some(Self::Tool),
            4 => Some(Self::Status),
            5 => Some(Self::Error),
            _ => None,
        }
    }
}

/// Chronological bounded activity with expandable details and an exact sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductActivity {
    sequence: u64,
    kind: ProductActivityKind,
    text: String,
    detail: String,
}
impl ProductActivity {
    /// Constructs a bounded safe-text projection; terminal clients must still sanitize controls.
    ///
    /// # Errors
    /// Rejects zero sequence, empty text, or oversized activity fields.
    pub fn new(
        sequence: u64,
        kind: ProductActivityKind,
        text: String,
        detail: String,
    ) -> Result<Self, ProductRunMessageError> {
        bounded_text(&text, MAX_PRODUCT_ACTIVITY_BYTES)?;
        if sequence == 0 || detail.len() > MAX_PRODUCT_ACTIVITY_BYTES {
            return Err(ProductRunMessageError::TooLong);
        }
        Ok(Self { sequence, kind, text, detail })
    }
    /// Stable ordering identity.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }
    /// Public activity family.
    #[must_use]
    pub const fn kind(&self) -> ProductActivityKind {
        self.kind
    }
    /// User-facing summary or assistant text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Expandable bounded details.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

/// Conversation-first projection alongside the unchanged legacy product snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductInteractionSnapshot {
    settlement: Option<peritus_run_settlement::RunSettlement>,
    snapshot: ProductRunSnapshot,
    mode: ProductInteractionMode,
    models: ProductRoleModels,
    received: u64,
    incorporated: u64,
    activities: Vec<ProductActivity>,
}
impl ProductInteractionSnapshot {
    /// Checks input ordering and bounded, strictly ordered activity retention.
    ///
    /// # Errors
    /// Rejects impossible acknowledgements, oversized collections, or non-monotonic sequences.
    pub fn new(
        snapshot: ProductRunSnapshot,
        mode: ProductInteractionMode,
        models: ProductRoleModels,
        received: u64,
        incorporated: u64,
        activities: Vec<ProductActivity>,
        settlement: Option<peritus_run_settlement::RunSettlement>,
    ) -> Result<Self, ProductRunMessageError> {
        if incorporated > received
            || activities.len() > MAX_PRODUCT_ACTIVITIES
            || activities.windows(2).any(|pair| pair[0].sequence() >= pair[1].sequence())
        {
            return Err(ProductRunMessageError::InvalidSettlement);
        }
        if let Some(settlement) = settlement {
            super::ProductRunSettlementSnapshot::new(snapshot.clone(), settlement)?;
        } else if snapshot.deliverable().is_some_and(|value| {
            value.qualification() != peritus_run_settlement::CandidateStage::Qualified
        }) {
            return Err(ProductRunMessageError::InvalidSettlement);
        }
        Ok(Self { settlement, snapshot, mode, models, received, incorporated, activities })
    }
    /// Verified terminal candidate facts, never inferred from conversation completion.
    #[must_use]
    pub const fn settlement(&self) -> Option<&peritus_run_settlement::RunSettlement> {
        self.settlement.as_ref()
    }
    /// Legacy-compatible run status and exact candidate facts.
    #[must_use]
    pub const fn snapshot(&self) -> &ProductRunSnapshot {
        &self.snapshot
    }
    /// Governing interaction mode.
    #[must_use]
    pub const fn mode(&self) -> ProductInteractionMode {
        self.mode
    }
    /// Governing model selections.
    #[must_use]
    pub const fn models(&self) -> &ProductRoleModels {
        &self.models
    }
    /// Latest durably received user revision.
    #[must_use]
    pub const fn received(&self) -> u64 {
        self.received
    }
    /// Latest revision incorporated into a provider request, not merely received by the daemon.
    #[must_use]
    pub const fn incorporated(&self) -> u64 {
        self.incorporated
    }
    /// Bounded ordered public activity. A first sequence above one signals omitted earlier history.
    #[must_use]
    pub fn activities(&self) -> &[ProductActivity] {
        &self.activities
    }
}

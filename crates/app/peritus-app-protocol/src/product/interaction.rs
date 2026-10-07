//! Additive conversation-first execution requests and honest user-input lifecycle observations.

use super::{ProductRoleModels, ProductRunMessageError, ProductRunSnapshot};
use peritus_types::{RunId, Sha256Digest};

/// Changes model choices for an existing conversation without adding user input or restarting it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductModelUpdate {
    run_id: peritus_types::RunId,
    models: ProductRoleModels,
}
impl ProductModelUpdate {
    /// Selects exact role models for subsequent logical model turns.
    #[must_use]
    pub const fn new(run_id: peritus_types::RunId, models: ProductRoleModels) -> Self {
        Self { run_id, models }
    }
    /// Conversation to update.
    #[must_use]
    pub const fn run_id(&self) -> peritus_types::RunId {
        self.run_id
    }
    /// Requested choices; the daemon validates and persists them before acknowledgement.
    #[must_use]
    pub const fn models(&self) -> &ProductRoleModels {
        &self.models
    }
}

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

/// Maximum retained public activity entries in one live conversation observation.
pub const MAX_PRODUCT_ACTIVITIES: usize = 256;
/// Maximum UTF-8 bytes in one field of a live activity preview.
pub const MAX_PRODUCT_ACTIVITY_BYTES: usize = 8192;
/// Maximum UTF-8 bytes in one independently addressable history segment field.
pub const MAX_PRODUCT_ACTIVITY_SEGMENT_BYTES: usize = 4096;
/// Maximum history segments returned by one interaction page.
pub const MAX_PRODUCT_ACTIVITY_PAGE_SEGMENTS: usize = 64;
/// Maximum older errors retained beside the evictable live activity window.
pub const MAX_PRODUCT_RETAINED_ERRORS: usize = 32;

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

/// Chronological activity with expandable details and an exact durable sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductActivity {
    sequence: u64,
    kind: ProductActivityKind,
    text: String,
    text_bytes: u64,
    detail: String,
    detail_bytes: u64,
}
impl ProductActivity {
    /// Constructs one complete logical activity; terminal clients must still sanitize controls.
    ///
    /// # Errors
    /// Rejects a zero sequence, empty text, or lengths that cannot be represented on the wire.
    pub fn new(
        sequence: u64,
        kind: ProductActivityKind,
        text: String,
        detail: String,
    ) -> Result<Self, ProductRunMessageError> {
        if sequence == 0 || text.is_empty() {
            return Err(if sequence == 0 {
                ProductRunMessageError::InvalidPage
            } else {
                ProductRunMessageError::Empty
            });
        }
        let text_bytes = u64::try_from(text.len()).map_err(|_| ProductRunMessageError::TooLong)?;
        let detail_bytes =
            u64::try_from(detail.len()).map_err(|_| ProductRunMessageError::TooLong)?;
        Ok(Self { sequence, kind, text, text_bytes, detail, detail_bytes })
    }

    /// Constructs a bounded prefix projection with explicit complete-field byte lengths.
    ///
    /// # Errors
    /// Rejects invalid identities, oversized prefixes, or lengths shorter than their prefixes.
    pub fn preview(
        sequence: u64,
        kind: ProductActivityKind,
        text: String,
        text_bytes: u64,
        detail: String,
        detail_bytes: u64,
    ) -> Result<Self, ProductRunMessageError> {
        let text_len = u64::try_from(text.len()).map_err(|_| ProductRunMessageError::TooLong)?;
        let detail_len = u64::try_from(detail.len()).map_err(|_| ProductRunMessageError::TooLong)?;
        if sequence == 0
            || text.is_empty()
            || text.len() > MAX_PRODUCT_ACTIVITY_BYTES
            || detail.len() > MAX_PRODUCT_ACTIVITY_BYTES
            || text_bytes < text_len
            || detail_bytes < detail_len
        {
            return Err(ProductRunMessageError::InvalidPage);
        }
        Ok(Self { sequence, kind, text, text_bytes, detail, detail_bytes })
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
    /// Complete logical text length, including any tail omitted from this live preview.
    #[must_use]
    pub const fn text_bytes(&self) -> u64 {
        self.text_bytes
    }
    /// Expandable detail text, complete for durable history and a prefix for live previews.
    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
    /// Complete logical detail length, including any tail omitted from this live preview.
    #[must_use]
    pub const fn detail_bytes(&self) -> u64 {
        self.detail_bytes
    }

    pub(crate) fn is_bounded_preview(&self) -> bool {
        self.text.len() <= MAX_PRODUCT_ACTIVITY_BYTES
            && self.detail.len() <= MAX_PRODUCT_ACTIVITY_BYTES
            && self.text_bytes >= u64::try_from(self.text.len()).unwrap_or(u64::MAX)
            && self.detail_bytes >= u64::try_from(self.detail.len()).unwrap_or(u64::MAX)
    }
}

/// Explicit accounting for the bounded live activity window of one history snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductActivityWindow {
    history: Sha256Digest,
    total: u64,
    omitted: u64,
    retained_errors: Vec<ProductActivity>,
    omitted_errors: u64,
    terminal_error: Option<ProductActivity>,
}

impl ProductActivityWindow {
    /// Creates exact omission accounting and separately retained error projections.
    ///
    /// # Errors
    /// Rejects oversized, unordered, non-error, or unbounded retained diagnostics.
    pub fn new(
        history: Sha256Digest,
        total: u64,
        omitted: u64,
        retained_errors: Vec<ProductActivity>,
        omitted_errors: u64,
        terminal_error: Option<ProductActivity>,
    ) -> Result<Self, ProductRunMessageError> {
        if omitted > total
            || retained_errors.len() > MAX_PRODUCT_RETAINED_ERRORS
            || retained_errors
                .iter()
                .any(|activity| activity.kind() != ProductActivityKind::Error
                    || !activity.is_bounded_preview()
                    || activity.sequence() > omitted)
            || retained_errors
                .windows(2)
                .any(|pair| pair[0].sequence() >= pair[1].sequence())
            || terminal_error.as_ref().is_some_and(|activity| {
                activity.kind() != ProductActivityKind::Error
                    || !activity.is_bounded_preview()
                    || total.checked_add(1) != Some(activity.sequence())
            })
        {
            return Err(ProductRunMessageError::InvalidPage);
        }
        Ok(Self { history, total, omitted, retained_errors, omitted_errors, terminal_error })
    }

    /// Digest binding every page and the live window to the same exact history contents.
    #[must_use]
    pub const fn history(&self) -> Sha256Digest {
        self.history
    }
    /// Highest durable logical activity sequence in this history snapshot.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.total
    }
    /// Number of sequence positions before the live window, including any legacy-retention gap.
    #[must_use]
    pub const fn omitted(&self) -> u64 {
        self.omitted
    }
    /// Older error activities retained independently of the evictable live window.
    #[must_use]
    pub fn retained_errors(&self) -> &[ProductActivity] {
        &self.retained_errors
    }
    /// Older error count omitted after applying the separate retained-error bound.
    #[must_use]
    pub const fn omitted_errors(&self) -> u64 {
        self.omitted_errors
    }
    /// Current non-durable stop diagnostic, if persistence itself failed.
    #[must_use]
    pub const fn terminal_error(&self) -> Option<&ProductActivity> {
        self.terminal_error.as_ref()
    }
}

/// Authoritative interaction projection for one governed product run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductInteractionSnapshot {
    settlement: Option<peritus_run_settlement::RunSettlement>,
    snapshot: ProductRunSnapshot,
    mode: ProductInteractionMode,
    models: ProductRoleModels,
    received: u64,
    incorporated: u64,
    activities: Vec<ProductActivity>,
    activity_window: Option<ProductActivityWindow>,
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
            || activities.iter().any(|activity| !activity.is_bounded_preview())
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
        Ok(Self {
            settlement,
            snapshot,
            mode,
            models,
            received,
            incorporated,
            activities,
            activity_window: None,
        })
    }
    /// Binds this bounded live view to complete history and explicit omission accounting.
    ///
    /// # Errors
    /// Rejects a window whose totals or contiguous sequence suffix disagree with the live view.
    pub fn with_activity_window(
        mut self,
        window: ProductActivityWindow,
    ) -> Result<Self, ProductRunMessageError> {
        let live = u64::try_from(self.activities.len()).map_err(|_| ProductRunMessageError::InvalidPage)?;
        let contiguous = self.activities.iter().enumerate().all(|(index, activity)| {
            u64::try_from(index)
                .ok()
                .and_then(|index| window.omitted().checked_add(index + 1))
                == Some(activity.sequence())
        });
        if window.omitted().checked_add(live) != Some(window.total()) || !contiguous {
            return Err(ProductRunMessageError::InvalidPage);
        }
        self.activity_window = Some(window);
        Ok(self)
    }
    /// Verified terminal candidate facts, never inferred from conversation completion.
    #[must_use]
    pub const fn settlement(&self) -> Option<&peritus_run_settlement::RunSettlement> {
        self.settlement.as_ref()
    }
    /// Run status and exact candidate facts.
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
    /// Exact history binding and omission accounting for this live view, when negotiated.
    #[must_use]
    pub const fn activity_window(&self) -> Option<&ProductActivityWindow> {
        self.activity_window.as_ref()
    }
}

/// Cursor for the segment immediately following one exact history position.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductActivityPageCursor {
    run_id: RunId,
    history: Sha256Digest,
    after_sequence: u64,
    after_segment: u64,
}

impl ProductActivityPageCursor {
    /// Creates a cursor bound to one run and exact history digest.
    ///
    /// # Errors
    /// Rejects the reserved zero activity sequence.
    pub fn new(
        run_id: RunId,
        history: Sha256Digest,
        after_sequence: u64,
        after_segment: u64,
    ) -> Result<Self, ProductRunMessageError> {
        if after_sequence == 0 {
            return Err(ProductRunMessageError::InvalidPage);
        }
        Ok(Self { run_id, history, after_sequence, after_segment })
    }
    /// Run whose history owns this cursor.
    #[must_use]
    pub const fn run_id(self) -> RunId {
        self.run_id
    }
    /// Exact history snapshot digest.
    #[must_use]
    pub const fn history(self) -> Sha256Digest {
        self.history
    }
    /// Durable activity containing the last emitted segment.
    #[must_use]
    pub const fn after_sequence(self) -> u64 {
        self.after_sequence
    }
    /// Zero-based segment ordinal last emitted within the activity.
    #[must_use]
    pub const fn after_segment(self) -> u64 {
        self.after_segment
    }
}

/// First-page or digest-bound continuation request for complete public activity history.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductActivityPageQuery {
    run_id: RunId,
    cursor: Option<ProductActivityPageCursor>,
}

impl ProductActivityPageQuery {
    /// Starts a fresh exact history snapshot for `run_id`.
    #[must_use]
    pub const fn first(run_id: RunId) -> Self {
        Self { run_id, cursor: None }
    }
    /// Continues the exact history snapshot represented by `cursor`.
    #[must_use]
    pub const fn after(cursor: ProductActivityPageCursor) -> Self {
        Self { run_id: cursor.run_id(), cursor: Some(cursor) }
    }
    /// Target run.
    #[must_use]
    pub const fn run_id(self) -> RunId {
        self.run_id
    }
    /// Continuation cursor, if this is not a first-page request.
    #[must_use]
    pub const fn cursor(self) -> Option<ProductActivityPageCursor> {
        self.cursor
    }
}

/// One bounded, offset-addressed UTF-8 segment of a complete logical activity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductActivitySegment {
    sequence: u64,
    segment: u64,
    kind: ProductActivityKind,
    text_offset: u64,
    text: String,
    text_bytes: u64,
    detail_offset: u64,
    detail: String,
    detail_bytes: u64,
}

impl ProductActivitySegment {
    /// Constructs a bounded exact segment with byte offsets into both logical fields.
    ///
    /// # Errors
    /// Rejects invalid offsets, empty segments, or fields above the physical segment bound.
    #[allow(clippy::too_many_arguments, reason = "both independently sliced fields need exact offsets and totals")]
    pub fn new(
        sequence: u64,
        segment: u64,
        kind: ProductActivityKind,
        text_offset: u64,
        text: String,
        text_bytes: u64,
        detail_offset: u64,
        detail: String,
        detail_bytes: u64,
    ) -> Result<Self, ProductRunMessageError> {
        let text_len = u64::try_from(text.len()).map_err(|_| ProductRunMessageError::TooLong)?;
        let detail_len = u64::try_from(detail.len()).map_err(|_| ProductRunMessageError::TooLong)?;
        if sequence == 0
            || text.len() > MAX_PRODUCT_ACTIVITY_SEGMENT_BYTES
            || detail.len() > MAX_PRODUCT_ACTIVITY_SEGMENT_BYTES
            || text.is_empty() && detail.is_empty()
            || text_offset.checked_add(text_len).is_none_or(|end| end > text_bytes)
            || detail_offset.checked_add(detail_len).is_none_or(|end| end > detail_bytes)
            || segment == 0 && (text_offset != 0 || detail_offset != 0)
            || segment != 0 && text_offset == 0 && detail_offset == 0
        {
            return Err(ProductRunMessageError::InvalidPage);
        }
        Ok(Self {
            sequence,
            segment,
            kind,
            text_offset,
            text,
            text_bytes,
            detail_offset,
            detail,
            detail_bytes,
        })
    }
    /// Durable logical activity sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 { self.sequence }
    /// Zero-based segment ordinal within the activity.
    #[must_use]
    pub const fn segment(&self) -> u64 { self.segment }
    /// Public activity family.
    #[must_use]
    pub const fn kind(&self) -> ProductActivityKind { self.kind }
    /// Exact byte offset of this text prefix within the complete activity text.
    #[must_use]
    pub const fn text_offset(&self) -> u64 { self.text_offset }
    /// UTF-8 text bytes carried by this segment.
    #[must_use]
    pub fn text(&self) -> &str { &self.text }
    /// Complete logical activity text length.
    #[must_use]
    pub const fn text_bytes(&self) -> u64 { self.text_bytes }
    /// Exact byte offset of this detail prefix within the complete activity detail.
    #[must_use]
    pub const fn detail_offset(&self) -> u64 { self.detail_offset }
    /// UTF-8 detail bytes carried by this segment.
    #[must_use]
    pub fn detail(&self) -> &str { &self.detail }
    /// Complete logical activity detail length.
    #[must_use]
    pub const fn detail_bytes(&self) -> u64 { self.detail_bytes }
}

/// Bounded page of exact retained activity segments plus its current live observation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductInteractionPage {
    query: ProductActivityPageQuery,
    interaction: ProductInteractionSnapshot,
    segments: Vec<ProductActivitySegment>,
    next: Option<ProductActivityPageCursor>,
}

impl ProductInteractionPage {
    /// Binds a page to its request, history snapshot, and exact next position.
    ///
    /// # Errors
    /// Rejects oversized, unordered, cross-run, or cursor-inconsistent pages.
    pub fn new(
        query: ProductActivityPageQuery,
        interaction: ProductInteractionSnapshot,
        segments: Vec<ProductActivitySegment>,
        next: Option<ProductActivityPageCursor>,
    ) -> Result<Self, ProductRunMessageError> {
        let Some(window) = interaction.activity_window() else {
            return Err(ProductRunMessageError::InvalidPage);
        };
        let cursor_matches = query.cursor().is_none_or(|cursor| {
            cursor.run_id() == query.run_id() && cursor.history() == window.history()
        });
        let first_follows = segments.first().is_none_or(|first| {
            query.cursor().map_or_else(
                || {
                    first.segment() == 0
                        && first.sequence() <= window.omitted().saturating_add(1)
                },
                |cursor| {
                    first.sequence() == cursor.after_sequence()
                        && first.segment() == cursor.after_segment().saturating_add(1)
                        || first.sequence() == cursor.after_sequence().saturating_add(1)
                            && first.segment() == 0
                },
            )
        });
        let contiguous = segments.windows(2).all(|pair| segment_follows(&pair[0], &pair[1]));
        let next_matches = next.is_none_or(|cursor| {
            segments.last().is_some_and(|last| {
                cursor.run_id() == query.run_id()
                    && cursor.history() == window.history()
                    && (cursor.after_sequence(), cursor.after_segment())
                        == (last.sequence(), last.segment())
            })
        });
        if interaction.snapshot().run_id() != query.run_id()
            || segments.len() > MAX_PRODUCT_ACTIVITY_PAGE_SEGMENTS
            || !cursor_matches
            || !first_follows
            || !contiguous
            || !next_matches
            || segments.iter().any(|segment| segment.sequence() > window.total())
            || segments.is_empty()
                && (query.cursor().is_some() || window.total() != 0 || next.is_some())
            || next.is_some()
                && segments.last().is_some_and(|last| {
                    last.sequence() == window.total()
                        && last.text_offset().saturating_add(
                            u64::try_from(last.text().len()).unwrap_or(u64::MAX),
                        ) == last.text_bytes()
                        && last.detail_offset().saturating_add(
                            u64::try_from(last.detail().len()).unwrap_or(u64::MAX),
                        ) == last.detail_bytes()
                })
            || next.is_none()
                && segments.last().is_some_and(|last| {
                    last.sequence() != window.total()
                        || last.text_offset().saturating_add(
                            u64::try_from(last.text().len()).unwrap_or(u64::MAX),
                        ) != last.text_bytes()
                        || last.detail_offset().saturating_add(
                            u64::try_from(last.detail().len()).unwrap_or(u64::MAX),
                        ) != last.detail_bytes()
                })
        {
            return Err(ProductRunMessageError::InvalidPage);
        }
        Ok(Self { query, interaction, segments, next })
    }
    /// Request whose exact position this page answers.
    #[must_use]
    pub const fn query(&self) -> ProductActivityPageQuery { self.query }
    /// Current bounded run observation bound to the same history digest.
    #[must_use]
    pub const fn interaction(&self) -> &ProductInteractionSnapshot { &self.interaction }
    /// Ordered exact UTF-8 segments.
    #[must_use]
    pub fn segments(&self) -> &[ProductActivitySegment] { &self.segments }
    /// Next exact history position, if more segments remain.
    #[must_use]
    pub const fn next(&self) -> Option<ProductActivityPageCursor> { self.next }
}

fn segment_follows(previous: &ProductActivitySegment, next: &ProductActivitySegment) -> bool {
    let previous_text_end = previous.text_offset().checked_add(
        u64::try_from(previous.text().len()).unwrap_or(u64::MAX),
    );
    let previous_detail_end = previous.detail_offset().checked_add(
        u64::try_from(previous.detail().len()).unwrap_or(u64::MAX),
    );
    if previous.sequence() == next.sequence() {
        return previous.segment().checked_add(1) == Some(next.segment())
            && previous.kind() == next.kind()
            && previous.text_bytes() == next.text_bytes()
            && previous.detail_bytes() == next.detail_bytes()
            && previous_text_end == Some(next.text_offset())
            && previous_detail_end == Some(next.detail_offset());
    }
    previous.sequence().checked_add(1) == Some(next.sequence())
        && next.segment() == 0
        && next.text_offset() == 0
        && next.detail_offset() == 0
        && previous_text_end == Some(previous.text_bytes())
        && previous_detail_end == Some(previous.detail_bytes())
}

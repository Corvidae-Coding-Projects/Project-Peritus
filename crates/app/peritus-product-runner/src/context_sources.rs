//! Exact daemon-owned sources exposed through a read-only, cursor-addressed model tool.

/// Maximum source descriptors returned by one host catalog page.
pub const MAX_CONTEXT_SOURCE_PAGE: usize = 128;
/// Maximum exact UTF-8 bytes returned by one host source read.
pub const MAX_CONTEXT_SOURCE_SLICE_BYTES: usize = 32 * 1024;

/// Semantic role of one retained body.
///
/// The role is carried separately from the body and its display label so assistant history can
/// never be mistaken for a governing user request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ContextSourceKind {
    /// Untrusted reference material selected as external context.
    ExternalContext,
    /// Exact user-authored request text with governing request authority.
    UserRequest,
    /// Exact prior public assistant output retained only as conversation history.
    AssistantHistory,
}
impl ContextSourceKind {
    /// Returns the stable developer-tool value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ExternalContext => "external_context",
            Self::UserRequest => "user_request",
            Self::AssistantHistory => "assistant_history",
        }
    }
}

/// One immutable source retained outside the target workspace and prompt context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextSource {
    ordinal: u64,
    label: String,
    digest: [u8; 32],
    bytes: u64,
    kind: ContextSourceKind,
    requires_read: bool,
}
impl ContextSource {
    /// Creates one positive-ordinal source descriptor.
    ///
    /// # Errors
    /// Rejects empty labels, controls, and ordinal zero.
    pub fn new(
        ordinal: u64,
        label: String,
        digest: [u8; 32],
        bytes: u64,
    ) -> Result<Self, String> {
        if ordinal == 0
            || label.trim().is_empty()
            || label.chars().any(char::is_control)
        {
            return Err("invalid context source descriptor".to_owned());
        }
        Ok(Self {
            ordinal,
            label,
            digest,
            bytes,
            kind: ContextSourceKind::ExternalContext,
            requires_read: true,
        })
    }
    /// Assigns the semantic role retained by this descriptor.
    #[must_use]
    pub const fn with_kind(mut self, kind: ContextSourceKind) -> Self {
        self.kind = kind;
        self
    }
    /// Records that the exact digest-and-length-bound body is already present in the host-built
    /// prompt. Only the host source projection can grant this credit.
    #[must_use]
    pub const fn with_prompt_body(mut self) -> Self {
        self.requires_read = false;
        self
    }
    #[must_use]
    pub const fn ordinal(&self) -> u64 { self.ordinal }
    #[must_use]
    pub fn label(&self) -> &str { &self.label }
    #[must_use]
    pub const fn digest(&self) -> &[u8; 32] { &self.digest }
    #[must_use]
    pub const fn bytes(&self) -> u64 { self.bytes }
    #[must_use]
    pub const fn kind(&self) -> ContextSourceKind { self.kind }
    #[must_use]
    pub const fn requires_read(&self) -> bool { self.requires_read }
}

/// One bounded catalog page. `next` is the last returned ordinal when another page exists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextSourcePage {
    sources: Vec<ContextSource>,
    next: Option<u64>,
}
impl ContextSourcePage {
    /// Validates bounded, strictly increasing positive ordinals and an exact continuation cursor.
    pub fn new(
        after: Option<u64>,
        sources: Vec<ContextSource>,
        next: Option<u64>,
    ) -> Result<Self, String> {
        if after == Some(0)
            || sources.len() > MAX_CONTEXT_SOURCE_PAGE
            || sources.windows(2).any(|pair| pair[0].ordinal >= pair[1].ordinal)
            || sources.first().is_some_and(|first| after.is_some_and(|after| first.ordinal <= after))
            || next.is_some_and(|next| sources.last().is_none_or(|last| next != last.ordinal))
        {
            return Err("invalid context source page".to_owned());
        }
        Ok(Self { sources, next })
    }
    #[must_use]
    pub fn sources(&self) -> &[ContextSource] { &self.sources }
    #[must_use]
    pub const fn next(&self) -> Option<u64> { self.next }
}

/// One exact UTF-8 source slice and its next byte offset.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContextSourceSlice {
    source: u64,
    offset: u64,
    text: String,
    next: Option<u64>,
}
impl ContextSourceSlice {
    /// Validates one bounded slice against its immutable source descriptor.
    pub fn new(
        source: &ContextSource,
        offset: u64,
        text: String,
        next: Option<u64>,
    ) -> Result<Self, String> {
        let end = offset
            .checked_add(u64::try_from(text.len()).map_err(|_| "source slice length overflow")?)
            .ok_or("source slice length overflow")?;
        let valid = if source.bytes == 0 {
            offset == 0 && text.is_empty() && next.is_none()
        } else {
            !text.is_empty()
                && text.len() <= MAX_CONTEXT_SOURCE_SLICE_BYTES
                && offset < source.bytes
                && end <= source.bytes
                && next == (end < source.bytes).then_some(end)
        };
        if !valid {
            return Err("invalid context source slice".to_owned());
        }
        Ok(Self { source: source.ordinal, offset, text, next })
    }
    #[must_use]
    pub const fn source(&self) -> u64 { self.source }
    #[must_use]
    pub const fn offset(&self) -> u64 { self.offset }
    #[must_use]
    pub fn text(&self) -> &str { &self.text }
    #[must_use]
    pub const fn next(&self) -> Option<u64> { self.next }
}

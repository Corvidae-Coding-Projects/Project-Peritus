//! Bounded sensitive text, multimodal input, replay state, and provider extensions.

use core::fmt;

use peritus_types::{ArtifactId, Sha256Digest};

use crate::{CanonicalJson, ExtensionName, ProtocolError, ProtocolErrorKind, ProtocolLimits};

/// Sensitive UTF-8 model content with an explicit byte bound.
#[derive(Clone, Eq, Ord, PartialEq, PartialOrd)]
pub struct BoundedText(String);

impl BoundedText {
    /// Creates nonempty bounded model text.
    ///
    /// # Errors
    ///
    /// Rejects empty text, NUL, or text wider than the supplied protocol limit.
    pub fn new(value: String, limits: ProtocolLimits) -> Result<Self, ProtocolError> {
        if value.is_empty() || value.len() > limits.max_text_bytes() || value.contains('\0') {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidContent,
                "text",
                "model text is empty, contains NUL, or exceeds its byte bound",
            ));
        }
        Ok(Self(value))
    }

    /// Borrows sensitive text for an authorized wire projection.
    #[must_use]
    pub fn expose_for_wire(&self) -> &str {
        &self.0
    }

    /// Returns the UTF-8 byte length.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the value is empty; checked instances are always nonempty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for BoundedText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundedText")
            .field("bytes", &self.0.len())
            .field("content", &"[redacted]")
            .finish()
    }
}

/// Supported input-media semantic kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaKind {
    /// Image input.
    Image,
    /// Audio input.
    Audio,
    /// Document input.
    Document,
}

/// Checked MIME media type without parameters.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct MediaType(String);

impl MediaType {
    /// Parses a lowercase canonical `type/subtype` value.
    ///
    /// # Errors
    ///
    /// Rejects parameters, invalid ASCII token characters, or missing type/subtype.
    pub fn new(mut value: String) -> Result<Self, ProtocolError> {
        value.make_ascii_lowercase();
        let canonical = value;
        let mut parts = canonical.split('/');
        let major = parts.next().unwrap_or_default();
        let minor = parts.next().unwrap_or_default();
        if canonical.len() > 128
            || major.is_empty()
            || minor.is_empty()
            || parts.next().is_some()
            || !major.bytes().all(mime_token)
            || !minor.bytes().all(mime_token)
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidContent,
                "media_type",
                "media type must be a bounded MIME type without parameters",
            ));
        }
        Ok(Self(canonical))
    }

    /// Borrows the canonical media type.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Semantics of a non-inline media reference.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaReferenceKind {
    /// HTTPS content reference resolved by the provider.
    HttpsUrl,
    /// Opaque provider-managed file identity.
    ProviderFile,
}

#[derive(Clone, Eq, PartialEq)]
enum MediaSource {
    Inline { bytes: Vec<u8>, digest: Sha256Digest },
    Reference { kind: MediaReferenceKind, value: String, digest: Option<Sha256Digest> },
    Artifact { artifact_id: ArtifactId, digest: Sha256Digest },
}

/// One bounded multimodal input with no ambient read authority.
#[derive(Clone, Eq, PartialEq)]
pub struct MediaInput {
    kind: MediaKind,
    media_type: MediaType,
    source: MediaSource,
}

impl MediaInput {
    /// Creates bounded inline media and records its exact digest.
    ///
    /// # Errors
    ///
    /// Rejects empty or oversized bytes.
    pub fn inline(
        kind: MediaKind,
        media_type: MediaType,
        bytes: Vec<u8>,
        limits: ProtocolLimits,
    ) -> Result<Self, ProtocolError> {
        if bytes.is_empty() || bytes.len() > limits.max_inline_media_bytes() {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidContent,
                "inline_media",
                "inline media is empty or exceeds its byte bound",
            ));
        }
        let digest = peritus_codec::sha256(&bytes);
        Ok(Self { kind, media_type, source: MediaSource::Inline { bytes, digest } })
    }

    /// Creates a bounded HTTPS or provider-file reference.
    ///
    /// # Errors
    ///
    /// Rejects malformed, control-containing, or oversized references.
    pub fn referenced(
        kind: MediaKind,
        media_type: MediaType,
        reference_kind: MediaReferenceKind,
        value: String,
        digest: Option<Sha256Digest>,
    ) -> Result<Self, ProtocolError> {
        let structurally_valid = match reference_kind {
            MediaReferenceKind::HttpsUrl => value.starts_with("https://"),
            MediaReferenceKind::ProviderFile => !value.contains("//"),
        };
        if value.is_empty()
            || value.len() > 8 * 1024
            || value.contains('\0')
            || value.chars().any(char::is_control)
            || !structurally_valid
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidContent,
                "media_reference",
                "media reference is malformed or exceeds its byte bound",
            ));
        }
        Ok(Self {
            kind,
            media_type,
            source: MediaSource::Reference { kind: reference_kind, value, digest },
        })
    }

    /// Creates an authenticated Peritus artifact reference without reading it.
    #[must_use]
    pub const fn artifact(
        kind: MediaKind,
        media_type: MediaType,
        artifact_id: ArtifactId,
        digest: Sha256Digest,
    ) -> Self {
        Self { kind, media_type, source: MediaSource::Artifact { artifact_id, digest } }
    }

    /// Returns the semantic media kind.
    #[must_use]
    pub const fn kind(&self) -> MediaKind {
        self.kind
    }

    /// Borrows the media type.
    #[must_use]
    pub const fn media_type(&self) -> &MediaType {
        &self.media_type
    }

    /// Returns inline bytes for authorized wire projection.
    #[must_use]
    pub fn inline_bytes_for_wire(&self) -> Option<&[u8]> {
        match &self.source {
            MediaSource::Inline { bytes, .. } => Some(bytes),
            MediaSource::Reference { .. } | MediaSource::Artifact { .. } => None,
        }
    }

    /// Returns a sensitive external reference for authorized wire projection.
    #[must_use]
    pub fn reference_for_wire(&self) -> Option<(MediaReferenceKind, &str)> {
        match &self.source {
            MediaSource::Reference { kind, value, .. } => Some((*kind, value)),
            MediaSource::Inline { .. } | MediaSource::Artifact { .. } => None,
        }
    }

    /// Returns the content digest when known.
    #[must_use]
    pub const fn digest(&self) -> Option<Sha256Digest> {
        match &self.source {
            MediaSource::Inline { digest, .. }
            | MediaSource::Artifact { digest, .. }
            | MediaSource::Reference { digest: Some(digest), .. } => Some(*digest),
            MediaSource::Reference { digest: None, .. } => None,
        }
    }

    /// Returns inline byte consumption.
    #[must_use]
    pub fn inline_len(&self) -> usize {
        self.inline_bytes_for_wire().map_or(0, <[u8]>::len)
    }

    pub(crate) const fn artifact_reference(&self) -> Option<(ArtifactId, Sha256Digest)> {
        match self.source {
            MediaSource::Artifact { artifact_id, digest } => Some((artifact_id, digest)),
            MediaSource::Inline { .. } | MediaSource::Reference { .. } => None,
        }
    }
}

impl fmt::Debug for MediaInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MediaInput")
            .field("kind", &self.kind)
            .field("media_type", &self.media_type)
            .field("bytes", &self.inline_len())
            .field("source", &"[redacted]")
            .finish()
    }
}

/// Sensitive opaque reasoning replay state plus an optional visible summary.
#[derive(Clone, Eq, PartialEq)]
pub struct ReasoningReplay {
    summary: Option<BoundedText>,
    opaque: Vec<u8>,
}

impl ReasoningReplay {
    /// Creates nonempty bounded replay state.
    ///
    /// # Errors
    ///
    /// Rejects empty or oversized opaque state.
    pub fn new(
        summary: Option<BoundedText>,
        opaque: Vec<u8>,
        limits: ProtocolLimits,
    ) -> Result<Self, ProtocolError> {
        if opaque.is_empty() || opaque.len() > limits.max_extension_bytes() {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidContent,
                "reasoning_replay",
                "reasoning replay state is empty or exceeds its byte bound",
            ));
        }
        Ok(Self { summary, opaque })
    }

    /// Borrows the optional visible summary.
    #[must_use]
    pub const fn summary(&self) -> Option<&BoundedText> {
        self.summary.as_ref()
    }

    /// Borrows sensitive replay bytes for an authorized provider projection.
    #[must_use]
    pub fn opaque_for_wire(&self) -> &[u8] {
        &self.opaque
    }
}

impl fmt::Debug for ReasoningReplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReasoningReplay")
            .field("summary", &self.summary.as_ref().map(BoundedText::len))
            .field("opaque_bytes", &self.opaque.len())
            .field("content", &"[redacted]")
            .finish()
    }
}

/// Explicitly capability-gated provider-native data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProviderExtension {
    name: ExtensionName,
    value: CanonicalJson,
}

impl ProviderExtension {
    /// Creates a named bounded extension.
    #[must_use]
    pub const fn new(name: ExtensionName, value: CanonicalJson) -> Self {
        Self { name, value }
    }

    /// Borrows the extension name.
    #[must_use]
    pub const fn name(&self) -> &ExtensionName {
        &self.name
    }

    /// Borrows the redacted canonical value.
    #[must_use]
    pub const fn value(&self) -> &CanonicalJson {
        &self.value
    }
}

/// Complete semantic content block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContentBlock {
    /// Text input/output.
    Text(BoundedText),
    /// Image input.
    Image(MediaInput),
    /// Audio input.
    Audio(MediaInput),
    /// Document input.
    Document(MediaInput),
    /// Completed assistant function call.
    ToolCall(crate::CompletedToolCall),
    /// Function result supplied by the application.
    ToolResult(crate::ToolResult),
    /// Explicit model refusal.
    Refusal(BoundedText),
    /// Provider reasoning state that must be replayed exactly.
    Reasoning(ReasoningReplay),
    /// Explicit provider-native extension.
    ProviderExtension(ProviderExtension),
}

impl ContentBlock {
    pub(crate) fn inline_media_bytes(&self) -> usize {
        match self {
            Self::Image(media) | Self::Audio(media) | Self::Document(media) => media.inline_len(),
            Self::Text(_)
            | Self::ToolCall(_)
            | Self::ToolResult(_)
            | Self::Refusal(_)
            | Self::Reasoning(_)
            | Self::ProviderExtension(_) => 0,
        }
    }
}

const fn mime_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(byte, b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-')
}

//! Bounded sensitive text, multimodal input, replay state, and provider extensions.

use core::fmt;

use peritus_types::{ArtifactId, Sha256Digest};

use crate::{
    CanonicalJson, ExtensionName, JsonBounds, ProtocolError, ProtocolErrorKind, ProtocolLimits,
};

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
        let text = Self(value);
        text.validate_under(limits)?;
        Ok(text)
    }

    pub(crate) fn validate_under(&self, limits: ProtocolLimits) -> Result<(), ProtocolError> {
        if self.0.is_empty()
            || self.0.len() > limits.max_text_bytes()
            || self.0.contains('\0')
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidContent,
                "text",
                "model text is empty, contains NUL, or exceeds its byte bound",
            ));
        }
        Ok(())
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
        if major.is_empty()
            || minor.is_empty()
            || parts.next().is_some()
            || !major.bytes().all(mime_token)
            || !minor.bytes().all(mime_token)
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidContent,
                "media_type",
                "media type must be a canonical MIME type without parameters",
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
    ResolvedArtifact {
        artifact_id: ArtifactId,
        digest: Sha256Digest,
        bytes: Vec<u8>,
    },
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
        let maximum = u64::try_from(limits.max_inline_media_bytes()).map_err(|_| {
            ProtocolError::at(
                ProtocolErrorKind::InvalidLimit,
                "inline_media",
                "inline media protocol limit is not representable",
            )
        })?;
        Self::inline_with_maximum(kind, media_type, bytes, maximum)
    }

    /// Creates inline media under an explicit provider-owned per-object byte limit.
    ///
    /// A later ModelRequest still applies its protocol representation bounds. Durable large media
    /// should use artifact references instead of this in-memory compatibility constructor.
    ///
    /// # Errors
    /// Rejects empty bytes, an unrepresentable length, or the explicit provider limit.
    pub fn inline_with_maximum(
        kind: MediaKind,
        media_type: MediaType,
        bytes: Vec<u8>,
        maximum_bytes: u64,
    ) -> Result<Self, ProtocolError> {
        let length = u64::try_from(bytes.len()).map_err(|_| {
            ProtocolError::at(
                ProtocolErrorKind::InvalidContent,
                "inline_media",
                "inline media length is not representable",
            )
        })?;
        if bytes.is_empty() || length > maximum_bytes {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidContent,
                "inline_media",
                "inline media is empty or exceeds its byte bound",
            ));
        }
        let digest = peritus_codec::sha256(&bytes);
        Ok(Self { kind, media_type, source: MediaSource::Inline { bytes, digest } })
    }

    /// Creates an HTTPS or provider-file reference.
    ///
    /// # Errors
    ///
    /// Rejects malformed or control-containing references. Enclosing transports own byte limits.
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
            || value.contains('\0')
            || value.chars().any(char::is_control)
            || !structurally_valid
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidContent,
                "media_reference",
                "media reference is malformed or contains control characters",
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

    /// Attaches exact transient provider bytes to an authenticated artifact reference.
    ///
    /// Canonical identity remains the artifact identity and digest. The resolved bytes exist only
    /// for the immediate provider projection and are never copied into request archives.
    ///
    /// # Errors
    /// Rejects a non-artifact source, empty bytes, a digest mismatch, or the selected provider's
    /// actual per-image byte limit.
    pub fn with_resolved_artifact(
        mut self,
        bytes: Vec<u8>,
        maximum_bytes: u64,
    ) -> Result<Self, ProtocolError> {
        let (artifact_id, digest) = self.artifact_reference().ok_or_else(|| {
            ProtocolError::at(
                ProtocolErrorKind::InvalidContent,
                "artifact_media",
                "only an authenticated artifact reference can be resolved",
            )
        })?;
        let length = u64::try_from(bytes.len()).map_err(|_| {
            ProtocolError::at(
                ProtocolErrorKind::InvalidContent,
                "artifact_media",
                "resolved artifact length is not representable",
            )
        })?;
        if bytes.is_empty()
            || length > maximum_bytes
            || peritus_codec::sha256(&bytes) != digest
        {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidContent,
                "artifact_media",
                "resolved artifact bytes do not match the exact provider-bound reference",
            ));
        }
        self.source = MediaSource::ResolvedArtifact { artifact_id, digest, bytes };
        Ok(self)
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

    /// Returns bytes for an authorized immediate provider projection.
    #[must_use]
    pub fn inline_bytes_for_wire(&self) -> Option<&[u8]> {
        match &self.source {
            MediaSource::Inline { bytes, .. } | MediaSource::ResolvedArtifact { bytes, .. } => {
                Some(bytes)
            }
            MediaSource::Reference { .. } | MediaSource::Artifact { .. } => None,
        }
    }

    /// Returns a sensitive external reference for authorized wire projection.
    #[must_use]
    pub fn reference_for_wire(&self) -> Option<(MediaReferenceKind, &str)> {
        match &self.source {
            MediaSource::Reference { kind, value, .. } => Some((*kind, value)),
            MediaSource::Inline { .. }
            | MediaSource::Artifact { .. }
            | MediaSource::ResolvedArtifact { .. } => None,
        }
    }

    /// Returns the content digest when known.
    #[must_use]
    pub const fn digest(&self) -> Option<Sha256Digest> {
        match &self.source {
            MediaSource::Inline { digest, .. }
            | MediaSource::Artifact { digest, .. }
            | MediaSource::ResolvedArtifact { digest, .. }
            | MediaSource::Reference { digest: Some(digest), .. } => Some(*digest),
            MediaSource::Reference { digest: None, .. } => None,
        }
    }

    /// Returns bytes stored inline in the semantic request.
    #[must_use]
    pub fn inline_len(&self) -> usize {
        match &self.source {
            MediaSource::Inline { bytes, .. } => bytes.len(),
            MediaSource::Reference { .. }
            | MediaSource::Artifact { .. }
            | MediaSource::ResolvedArtifact { .. } => 0,
        }
    }

    pub(crate) fn artifact_reference(&self) -> Option<(ArtifactId, Sha256Digest)> {
        match &self.source {
            MediaSource::Artifact { artifact_id, digest }
            | MediaSource::ResolvedArtifact { artifact_id, digest, .. } => {
                Some((*artifact_id, *digest))
            }
            MediaSource::Inline { .. } | MediaSource::Reference { .. } => None,
        }
    }

    pub(crate) fn validate_under(&self, limits: ProtocolLimits) -> Result<(), ProtocolError> {
        if matches!(
            &self.source,
            MediaSource::Inline { bytes, .. } if bytes.len() > limits.max_inline_media_bytes()
        ) {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidContent,
                "inline_media",
                "inline media exceeds the selected protocol byte bound",
            ));
        }
        Ok(())
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
        let replay = Self { summary, opaque };
        replay.validate_under(limits)?;
        Ok(replay)
    }

    pub(crate) fn validate_under(&self, limits: ProtocolLimits) -> Result<(), ProtocolError> {
        if let Some(summary) = &self.summary {
            summary.validate_under(limits)?;
        }
        if self.opaque.is_empty() || self.opaque.len() > limits.max_extension_bytes() {
            return Err(ProtocolError::at(
                ProtocolErrorKind::InvalidContent,
                "reasoning_replay",
                "reasoning replay state is empty or exceeds its byte bound",
            ));
        }
        Ok(())
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

    pub(crate) fn validate_under(&self, limits: ProtocolLimits) -> Result<(), ProtocolError> {
        self.value.validate_under(JsonBounds::extension(limits))
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

    pub(crate) fn validate_under(&self, limits: ProtocolLimits) -> Result<(), ProtocolError> {
        match self {
            Self::Text(value) | Self::Refusal(value) => value.validate_under(limits),
            Self::Image(media) | Self::Audio(media) | Self::Document(media) => {
                media.validate_under(limits)
            }
            Self::ToolCall(call) => call.validate_under(limits),
            Self::ToolResult(result) => result.validate_under(limits),
            Self::Reasoning(replay) => replay.validate_under(limits),
            Self::ProviderExtension(extension) => extension.validate_under(limits),
        }
    }
}

const fn mime_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(byte, b'!' | b'#' | b'$' | b'&' | b'^' | b'_' | b'.' | b'+' | b'-')
}

//! Canonical field and nested-type metadata for the schema-v1 JSON projection.

/// Canonical binary representation of one field.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CanonicalWireType {
    /// One canonical boolean octet.
    Boolean,
    /// An unsigned integer of the named width.
    U8,
    /// An unsigned integer of the named width.
    U16,
    /// An unsigned integer of the named width.
    U32,
    /// An unsigned integer of the named width.
    U64,
    /// A signed 32-bit integer encoded in network byte order.
    I32,
    /// A fixed-width 16-byte nominal identifier.
    Identifier,
    /// A fixed-width SHA-256 digest.
    Digest,
    /// A length-prefixed UTF-8 string.
    Utf8,
    /// A length-prefixed byte string.
    Bytes,
    /// A length-prefixed, ordered sequence.
    Sequence,
    /// An option tag followed by the value when present.
    Option,
    /// An ordered aggregate with no implicit padding.
    Struct,
}

impl CanonicalWireType {
    /// Returns the stable registry spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Boolean => "bool/u8",
            Self::U8 => "u8",
            Self::U16 => "u16-be",
            Self::U32 => "u32-be",
            Self::U64 => "u64-be",
            Self::I32 => "i32-be",
            Self::Identifier => "fixed[16]",
            Self::Digest => "fixed[32]",
            Self::Utf8 => "len+utf8",
            Self::Bytes => "len+bytes",
            Self::Sequence => "len+items",
            Self::Option => "option+value",
            Self::Struct => "ordered-fields",
        }
    }
}

/// A semantic or negotiated ceiling applying to one field.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FieldBound {
    /// Zero is not a valid value.
    NonZero,
    /// The codec frame-byte ceiling applies.
    CodecFrameBytes,
    /// The codec collection-item ceiling applies.
    CodecCollectionItems,
    /// The codec UTF-8 byte ceiling applies.
    CodecStringBytes,
    /// The codec opaque-byte ceiling applies.
    CodecOpaqueBytes,
    /// The negotiated version-range ceiling applies.
    Versions,
    /// The negotiated feature-count ceiling applies.
    Features,
    /// The fixed 128-byte idempotency-key ceiling applies.
    IdempotencyKeyBytes,
    /// The negotiated topic-count ceiling applies.
    Topics,
    /// The negotiated in-flight delivery ceiling applies.
    InFlightEvents,
    /// The negotiated artifact-chunk byte ceiling applies.
    ArtifactChunkBytes,
    /// The negotiated prompt-choice ceiling applies.
    PromptChoices,
    /// The negotiated terminal-chunk byte ceiling applies.
    TerminalChunkBytes,
    /// The negotiated diagnostic byte ceiling applies.
    DiagnosticBytes,
    /// The negotiated remaining-work ceiling applies.
    RemainingWorkItems,
    /// Items must be strictly sorted and unique.
    SortedUnique,
    /// Cursors, offsets, or sequence numbers must be contiguous.
    Contiguous,
    /// Bytes must conserve the declared artifact size.
    DeclaredArtifactSize,
    /// A repeated identity must agree with its enclosing envelope.
    EnvelopeBinding,
}

impl FieldBound {
    /// Returns the stable registry spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NonZero => "nonzero",
            Self::CodecFrameBytes => "codec.max-frame-bytes",
            Self::CodecCollectionItems => "codec.max-collection-items",
            Self::CodecStringBytes => "codec.max-string-bytes",
            Self::CodecOpaqueBytes => "codec.max-opaque-bytes",
            Self::Versions => "app.max-versions",
            Self::Features => "app.max-features",
            Self::IdempotencyKeyBytes => "128 bytes",
            Self::Topics => "app.max-topics",
            Self::InFlightEvents => "app.max-in-flight-events",
            Self::ArtifactChunkBytes => "app.max-artifact-chunk-bytes",
            Self::PromptChoices => "app.max-prompt-choices",
            Self::TerminalChunkBytes => "app.max-terminal-chunk-bytes",
            Self::DiagnosticBytes => "app.max-diagnostic-bytes",
            Self::RemainingWorkItems => "app.max-remaining-work-items",
            Self::SortedUnique => "strictly-sorted-unique",
            Self::Contiguous => "contiguous",
            Self::DeclaredArtifactSize => "declared-artifact-size",
            Self::EnvelopeBinding => "envelope-binding",
        }
    }
}

/// JSON representation used by the documented lossless projection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum JsonShape {
    /// JSON boolean.
    Boolean,
    /// Nonnegative JSON integer that is safe in JavaScript.
    U16,
    /// Nonnegative JSON integer that is safe in JavaScript.
    U32,
    /// Signed 32-bit JSON integer.
    I32,
    /// Decimal string preserving every unsigned 64-bit value.
    U64String,
    /// UTF-8 JSON string.
    String,
    /// Base64 string preserving exact bytes.
    Base64,
    /// Lowercase, nonzero, 16-byte hexadecimal identifier.
    Identifier,
    /// Lowercase 32-byte hexadecimal SHA-256 digest.
    Digest,
    /// Closed string enumeration.
    Enum(&'static [&'static str]),
    /// Reference to one named nested type.
    Ref(&'static str),
    /// Ordered array of one named nested type.
    ArrayRef(&'static str),
    /// Ordered array of strings.
    StringArray,
}

/// One field in canonical wire order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AppFieldDescriptor {
    /// Camel-case name in the typed JSON projection.
    pub name: &'static str,
    /// Canonical binary representation.
    pub wire_type: CanonicalWireType,
    /// Applicable semantic and resource bounds.
    pub bounds: &'static [FieldBound],
    /// Production Rust type.
    pub rust_type: &'static str,
    /// TypeScript representation.
    pub typescript_type: &'static str,
    /// JSON representation.
    pub json_shape: JsonShape,
    /// Whether the JSON object must contain the field.
    pub required: bool,
}

/// One ordered nested application-protocol type.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AppTypeDescriptor {
    /// Stable JSON Schema definition and TypeScript name.
    pub name: &'static str,
    /// Production Rust type.
    pub rust_type: &'static str,
    /// Fields in canonical wire order.
    pub fields: &'static [AppFieldDescriptor],
}

pub(super) const fn field(
    name: &'static str,
    wire_type: CanonicalWireType,
    bounds: &'static [FieldBound],
    rust_type: &'static str,
    typescript_type: &'static str,
    json_shape: JsonShape,
    required: bool,
) -> AppFieldDescriptor {
    AppFieldDescriptor { name, wire_type, bounds, rust_type, typescript_type, json_shape, required }
}

mod flows;
mod types;

pub use flows::APP_FLOW_TYPES;
pub use types::APP_NESTED_TYPES;

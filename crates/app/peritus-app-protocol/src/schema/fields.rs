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
    /// At most 4096 UTF-8 bytes in one inert launch/profile text field.
    WorkbenchLaunchTextBytes,
    /// At most 64 KiB in one explicit preview input write.
    WorkbenchPreviewInputBytes,
    /// Maximum literal arguments in one launch profile.
    WorkbenchLaunchArguments,
    /// Maximum inherited environment references in one launch profile.
    WorkbenchLaunchEnvironment,
    /// Maximum retained launch rows in one result page.
    WorkbenchLaunches,
    /// Maximum retained input receipts for one launch.
    WorkbenchInteractions,
    /// Maximum retained selected-window captures for one launch.
    WorkbenchCaptures,
    /// Maximum retained capture-anchored feedback rows for one launch.
    WorkbenchArtifactFeedback,
    /// Maximum finite wall duration for one preview process.
    WorkbenchLaunchWallMillis,
    /// Maximum 4096 UTF-8 bytes for an inert workspace-relative path.
    WorkbenchFilePathBytes,
    /// Maximum 32 file references per page.
    WorkbenchFilePage,
    /// Maximum 256 retained file references.
    WorkbenchFileHistory,
    /// At most 4 MiB of original encoded image bytes.
    WorkbenchImageBytes,
    /// At most 1024 UTF-8 source label bytes.
    WorkbenchImageLabelBytes,
    /// At most 8192 pixels per image side.
    WorkbenchImageSide,
    /// At most 64 complete image frames.
    WorkbenchImageFrames,
    /// Maximum image references returned in one revision-fenced page.
    WorkbenchImagePage,
    /// At most 32 exact input rows per revision-fenced page.
    WorkbenchInputPage,
    /// At most 1024 input identities or historical content revisions.
    WorkbenchInputs,
    /// At most 32 prerequisite identities for one input.
    WorkbenchInputDependencies,
    /// At most 8192 bytes of exact inert input text.
    WorkbenchInputBytes,
    /// Maximum context metadata rows in a page.
    WorkbenchContextPage,
    /// Maximum context metadata rows in one complete view.
    WorkbenchContextRows,
    /// Maximum bytes described by one context row.
    WorkbenchContextSourceBytes,
    /// Maximum explicit user-confirmed brief fields.
    WorkbenchBriefFields,
    /// Maximum agent-proposed public replies shown in a brief.
    WorkbenchBriefProposals,
    /// Maximum host-observed attachment facts shown in a brief.
    WorkbenchBriefObservations,
    /// Maximum public-reply source handles in one compaction preview.
    WorkbenchCompactionEntries,
    /// Maximum UTF-8 bytes in an optional compaction focus.
    WorkbenchCompactionFocusBytes,
    /// Maximum typed completion criteria in one persistent goal.
    WorkbenchGoalCriteria,
    /// The three stable persistent-goal accounting roles.
    WorkbenchGoalRoles,
    /// Maximum UTF-8 bytes in one persistent-goal transition reason.
    WorkbenchGoalReasonBytes,
    /// Maximum exact covered paths, exclusions, or effect notices in one checkpoint exchange.
    WorkbenchCheckpointPaths,
    /// Maximum UTF-8 bytes in a nonempty inert checkpoint name.
    WorkbenchCheckpointNameBytes,
    /// Maximum UTF-8 bytes in one checkpoint exclusion or effect notice.
    WorkbenchCheckpointTextBytes,
    /// Maximum UTF-8 bytes in one terminal restore path or diagnostic.
    WorkbenchRestoreTextBytes,
    /// At most 256 UTF-8 bytes in a nonempty inert conversation title.
    ConversationTitleBytes,
    /// Maximum literal local conversation-search text.
    ConversationSearchBytes,
    /// Maximum exact source-linked conversation snippet.
    ConversationSnippetBytes,
    /// Maximum conversation handoff summary.
    ConversationHandoffBytes,
    /// Maximum conversations in one library page.
    ConversationLibraryPage,
    /// At most 32 unique classified findings.
    DoctorFindings,
    /// At most 64 UTF-8 bytes in an inert check label.
    DoctorCheckBytes,
    /// At most 1024 UTF-8 bytes in inert diagnostic text.
    DoctorTextBytes,
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
    /// The product-run list ceiling applies.
    ProductRuns,
    /// The product task byte ceiling applies.
    ProductTaskBytes,
    /// The product detail byte ceiling applies.
    ProductDetailBytes,
    /// The product deliverable changed-path ceiling applies.
    ProductDeliverablePaths,
    /// The product deliverable successful-command ceiling applies.
    ProductDeliverableCommands,
    /// Interactive activity-entry ceiling.
    ProductActivities,
    /// Interactive activity text and detail byte ceiling.
    ProductActivityBytes,
    /// Provider catalog entry ceiling.
    ProductModels,
    /// Model identifier or label byte ceiling.
    ProductModelBytes,
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
            Self::WorkbenchLaunchTextBytes => "workbench.max-launch-text-bytes (4096)",
            Self::WorkbenchPreviewInputBytes => "workbench.max-preview-input-bytes (65536)",
            Self::WorkbenchLaunchArguments => "workbench.max-launch-arguments (256)",
            Self::WorkbenchLaunchEnvironment => "workbench.max-launch-environment (64)",
            Self::WorkbenchLaunches => "workbench.max-launches (16)",
            Self::WorkbenchInteractions => "workbench.max-preview-interactions (256)",
            Self::WorkbenchCaptures => "workbench.max-preview-captures (64)",
            Self::WorkbenchArtifactFeedback => "workbench.max-artifact-feedback (256)",
            Self::WorkbenchLaunchWallMillis => "workbench.max-launch-wall-millis (600000)",
            Self::WorkbenchImageBytes => "workbench.max-image-bytes (4194304)",
            Self::WorkbenchFilePathBytes => "workbench.max-file-path-bytes (4096)",
            Self::WorkbenchFilePage => "workbench.max-file-page (32)",
            Self::WorkbenchFileHistory => "workbench.max-file-history (256)",
            Self::WorkbenchImageLabelBytes => "workbench.max-image-label-bytes (1024)",
            Self::WorkbenchImageSide => "workbench.max-image-side (8192)",
            Self::WorkbenchImageFrames => "workbench.max-image-frames (64)",
            Self::WorkbenchImagePage => "workbench.max-image-page (32)",
            Self::WorkbenchInputPage => "workbench.max-input-page (32)",
            Self::WorkbenchInputs => "workbench.max-inputs (1024)",
            Self::WorkbenchInputDependencies => "workbench.max-input-dependencies (32)",
            Self::WorkbenchInputBytes => "workbench.max-input-bytes (8192)",
            Self::WorkbenchContextPage => "workbench.max-context-page (32)",
            Self::WorkbenchContextRows => "workbench.max-context-rows (8192)",
            Self::WorkbenchContextSourceBytes => "workbench.max-context-source-bytes (67108864)",
            Self::WorkbenchBriefFields => "workbench.max-brief-fields (4)",
            Self::WorkbenchBriefProposals => "workbench.max-brief-proposals (8)",
            Self::WorkbenchBriefObservations => "workbench.max-brief-observations (32)",
            Self::WorkbenchCompactionEntries => "workbench.max-compaction-entries (1024)",
            Self::WorkbenchCompactionFocusBytes => "workbench.max-compaction-focus-bytes (1024)",
            Self::WorkbenchGoalCriteria => "workbench.max-goal-criteria (16)",
            Self::WorkbenchGoalRoles => "workbench.goal-roles (3)",
            Self::WorkbenchGoalReasonBytes => "workbench.max-goal-reason-bytes (512)",
            Self::WorkbenchCheckpointPaths => "workbench.max-checkpoint-paths (64)",
            Self::WorkbenchCheckpointNameBytes => "workbench.max-checkpoint-name-bytes (256)",
            Self::WorkbenchCheckpointTextBytes => "workbench.max-checkpoint-text-bytes (512)",
            Self::WorkbenchRestoreTextBytes => "workbench.max-restore-text-bytes (4096)",
            Self::ConversationTitleBytes => "workbench.max-title-bytes (256)",
            Self::ConversationSearchBytes => "workbench.max-conversation-search-bytes (256)",
            Self::ConversationSnippetBytes => "workbench.max-conversation-snippet-bytes (512)",
            Self::ConversationHandoffBytes => "workbench.max-conversation-handoff-bytes (1024)",
            Self::ConversationLibraryPage => "workbench.max-conversation-library-page (64)",
            Self::DoctorFindings => "doctor.max-findings (32)",
            Self::DoctorCheckBytes => "doctor.max-check-bytes (64)",
            Self::DoctorTextBytes => "doctor.max-text-bytes (1024)",
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
            Self::ProductRuns => "product.max-runs",
            Self::ProductTaskBytes => "product.max-task-bytes",
            Self::ProductDetailBytes => "product.max-detail-bytes",
            Self::ProductDeliverablePaths => "product.max-deliverable-paths",
            Self::ProductDeliverableCommands => "product.max-deliverable-commands",
            Self::ProductActivities => "product.max-activities (256)",
            Self::ProductActivityBytes => "product.max-activity-bytes (8192)",
            Self::ProductModels => "product.max-models (4096)",
            Self::ProductModelBytes => "product.max-model-bytes (512)",
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
    /// One of several closed named aggregate shapes, selected by a preceding wire tag.
    OneOfRef(&'static [&'static str]),
    /// Ordered array of one named nested type.
    ArrayRef(&'static str),
    /// Ordered array whose items are one of several closed named aggregate shapes.
    OneOfArrayRef(&'static [&'static str]),
    /// Ordered array of strings.
    StringArray,
    /// Ordered array of exact nominal identifiers.
    IdentifierArray,
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

#[cfg(test)]
mod tests;

//! Referenced product-run artifacts and immutable deliverable indexes.

use super::{AppTypeDescriptor, B, J, W, field};

const INDEX_KINDS: &[&str] = &["changed-paths", "successful-commands"];
const OPERATION_KINDS: &[&str] = &["execution", "commit", "discard", "command"];
const OPERATION_STATES: &[&str] = &[
    "running",
    "waiting-for-user",
    "succeeded",
    "failed",
    "cancelled",
    "recovery-required",
    "outcome-unknown",
];
const PHASES: &[&str] = &[
    "queued",
    "writing",
    "checking",
    "reviewing",
    "fixing",
    "verifying",
    "complete",
    "failed",
    "cancelled",
    "recovery-required",
    "waiting-for-user",
    "designing",
];
const STAGES: &[&str] =
    &["observed", "changed", "self-checked", "gates-passed", "review-pending", "qualified"];

pub(in crate::schema::fields) const ARTIFACT_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "ProductArtifactReference",
        rust_type: "ProductArtifactReference",
        fields: &[
            field("digest", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
            field("bytes", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true),
        ],
    },
    AppTypeDescriptor {
        name: "ProductDeliverableIndexReference",
        rust_type: "ProductDeliverableIndexReference",
        fields: &[
            field("root", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
            field("count", W::U64, &[], "u64", "UInt64", J::U64String, true),
        ],
    },
    AppTypeDescriptor {
        name: "ProductDeliverableReference",
        rust_type: "ProductDeliverableReference",
        fields: &[
            field("workspacePath", W::Struct, &[], "ProductArtifactReference", "ProductArtifactReference", J::Ref("ProductArtifactReference"), true),
            field("changedPaths", W::Struct, &[], "ProductDeliverableIndexReference", "ProductDeliverableIndexReference", J::Ref("ProductDeliverableIndexReference"), true),
            field("successfulCommands", W::Struct, &[], "ProductDeliverableIndexReference", "ProductDeliverableIndexReference", J::Ref("ProductDeliverableIndexReference"), true),
            field("runInstructions", W::Struct, &[], "ProductArtifactReference", "ProductArtifactReference", J::Ref("ProductArtifactReference"), true),
            field("qualification", W::U16, &[], "CandidateStage", "CandidateStage", J::Enum(STAGES), true),
            field("accepted", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            field("commitRevision", W::Option, &[], "Option<ProductArtifactReference>", "ProductArtifactReference", J::Ref("ProductArtifactReference"), false),
            field("exportPath", W::Option, &[], "Option<ProductArtifactReference>", "ProductArtifactReference", J::Ref("ProductArtifactReference"), false),
            field("discarded", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
        ],
    },
    AppTypeDescriptor {
        name: "ProductRunOperationReference",
        rust_type: "ProductRunOperationReference",
        fields: &[
            field("kind", W::U16, &[], "ProductRunOperationKind", "ProductRunOperationKind", J::Enum(OPERATION_KINDS), true),
            field("state", W::U16, &[], "ProductRunOperationState", "ProductRunOperationState", J::Enum(OPERATION_STATES), true),
            field("identity", W::Struct, &[], "ProductArtifactReference", "ProductArtifactReference", J::Ref("ProductArtifactReference"), true),
            field("known", W::Struct, &[], "ProductArtifactReference", "ProductArtifactReference", J::Ref("ProductArtifactReference"), true),
            field("uncertainty", W::Option, &[], "Option<ProductArtifactReference>", "ProductArtifactReference", J::Ref("ProductArtifactReference"), false),
            field("legalControls", W::Struct, &[], "ProductRunLegalControls", "ProductRunLegalControls", J::Ref("ProductRunLegalControls"), true),
        ],
    },
    AppTypeDescriptor {
        name: "ProductRunReferenceSnapshot",
        rust_type: "ProductRunReferenceSnapshot",
        fields: &[
            field("runId", W::Identifier, &[B::NonZero], "RunId", "RunId", J::Identifier, true),
            field("workspaceId", W::Identifier, &[B::NonZero], "WorkspaceId", "WorkspaceId", J::Identifier, true),
            field("providers", W::Struct, &[], "ProductProviderSelection", "ProductProviderSelection", J::Ref("ProductProviderSelection"), true),
            field("phase", W::U16, &[], "ProductRunPhase", "ProductRunPhase", J::Enum(PHASES), true),
            field("cycle", W::U32, &[], "u32", "number", J::U32, true),
            field("task", W::Struct, &[], "ProductArtifactReference", "ProductArtifactReference", J::Ref("ProductArtifactReference"), true),
            field("status", W::Struct, &[], "ProductArtifactReference", "ProductArtifactReference", J::Ref("ProductArtifactReference"), true),
            field("diff", W::Option, &[], "Option<ProductArtifactReference>", "ProductArtifactReference", J::Ref("ProductArtifactReference"), false),
            field("gates", W::Option, &[], "Option<ProductArtifactReference>", "ProductArtifactReference", J::Ref("ProductArtifactReference"), false),
            field("review", W::Option, &[], "Option<ProductArtifactReference>", "ProductArtifactReference", J::Ref("ProductArtifactReference"), false),
            field("summary", W::Option, &[], "Option<ProductArtifactReference>", "ProductArtifactReference", J::Ref("ProductArtifactReference"), false),
            field("operation", W::Struct, &[], "ProductRunOperationReference", "ProductRunOperationReference", J::Ref("ProductRunOperationReference"), true),
            field("deliverable", W::Option, &[], "Option<ProductDeliverableReference>", "ProductDeliverableReference", J::Ref("ProductDeliverableReference"), false),
            field("settlement", W::Option, &[], "Option<RunSettlement>", "RunSettlement", J::Ref("RunSettlement"), false),
        ],
    },
    AppTypeDescriptor {
        name: "ProductRunReferenceQuery",
        rust_type: "ProductRunReferenceQuery",
        fields: &[
            field("runId", W::Option, &[], "Option<RunId>", "RunId", J::Identifier, false),
            field("cursor", W::Option, &[], "Option<ProductRunPageCursor>", "ProductRunPageCursor", J::Ref("ProductRunPageCursor"), false),
        ],
    },
    AppTypeDescriptor {
        name: "ProductRunReferencePageEntry",
        rust_type: "ProductRunReferencePageEntry",
        fields: &[
            field("sequence", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true),
            field("snapshot", W::Struct, &[], "ProductRunReferenceSnapshot", "ProductRunReferenceSnapshot", J::Ref("ProductRunReferenceSnapshot"), true),
        ],
    },
    AppTypeDescriptor {
        name: "ProductRunReferencePage",
        rust_type: "ProductRunReferencePage",
        fields: &[
            field("store", W::Identifier, &[B::NonZero], "ProductRunStoreId", "ProductRunStoreId", J::Identifier, true),
            field("entries", W::Sequence, &[B::ProductRuns], "Vec<ProductRunReferencePageEntry>", "readonly ProductRunReferencePageEntry[]", J::ArrayRef("ProductRunReferencePageEntry"), true),
            field("next", W::Option, &[], "Option<ProductRunPageCursor>", "ProductRunPageCursor", J::Ref("ProductRunPageCursor"), false),
        ],
    },
    AppTypeDescriptor {
        name: "ProductArtifactQuery",
        rust_type: "ProductArtifactQuery",
        fields: &[
            field("runId", W::Identifier, &[B::NonZero], "RunId", "RunId", J::Identifier, true),
            field("source", W::Struct, &[], "ProductArtifactReference", "ProductArtifactReference", J::Ref("ProductArtifactReference"), true),
            field("offset", W::U64, &[B::Contiguous], "u64", "UInt64", J::U64String, true),
        ],
    },
    AppTypeDescriptor {
        name: "ProductArtifactPage",
        rust_type: "ProductArtifactPage",
        fields: &[
            field("query", W::Struct, &[], "ProductArtifactQuery", "ProductArtifactQuery", J::Ref("ProductArtifactQuery"), true),
            field("bytes", W::Bytes, &[B::ProductArtifactChunkBytes, B::ArtifactChunkBytes, B::CodecOpaqueBytes], "Vec<u8>", "string", J::Base64, true),
            field("next", W::Option, &[B::Contiguous], "Option<u64>", "UInt64", J::U64String, false),
        ],
    },
    AppTypeDescriptor {
        name: "ProductDeliverableIndexQuery",
        rust_type: "ProductDeliverableIndexQuery",
        fields: &[
            field("runId", W::Identifier, &[B::NonZero], "RunId", "RunId", J::Identifier, true),
            field("kind", W::U16, &[], "ProductDeliverableIndexKind", "ProductDeliverableIndexKind", J::Enum(INDEX_KINDS), true),
            field("index", W::Struct, &[], "ProductDeliverableIndexReference", "ProductDeliverableIndexReference", J::Ref("ProductDeliverableIndexReference"), true),
            field("after", W::Option, &[B::Contiguous], "Option<u64>", "UInt64", J::U64String, false),
        ],
    },
    AppTypeDescriptor {
        name: "ProductDeliverableIndexEntry",
        rust_type: "ProductDeliverableIndexEntry",
        fields: &[
            field("ordinal", W::U64, &[B::Contiguous], "u64", "UInt64", J::U64String, true),
            field("value", W::Struct, &[], "ProductArtifactReference", "ProductArtifactReference", J::Ref("ProductArtifactReference"), true),
        ],
    },
    AppTypeDescriptor {
        name: "ProductDeliverableIndexPage",
        rust_type: "ProductDeliverableIndexPage",
        fields: &[
            field("query", W::Struct, &[], "ProductDeliverableIndexQuery", "ProductDeliverableIndexQuery", J::Ref("ProductDeliverableIndexQuery"), true),
            field("entries", W::Sequence, &[B::ProductDeliverableIndexPage], "Vec<ProductDeliverableIndexEntry>", "readonly ProductDeliverableIndexEntry[]", J::ArrayRef("ProductDeliverableIndexEntry"), true),
            field("next", W::Option, &[B::Contiguous], "Option<u64>", "UInt64", J::U64String, false),
        ],
    },
];

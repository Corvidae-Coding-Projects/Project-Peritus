//! Covered-path checkpoints, exact rewind previews, and durable restore receipts.

use super::super::{
    AppFieldDescriptor, AppTypeDescriptor, CanonicalWireType as W, FieldBound as B, JsonShape as J,
    field,
};

const VERSION_TYPES: &[&str] =
    &["WorkbenchCheckpointAbsentVersion", "WorkbenchCheckpointPresentVersion"];

const fn nested(name: &'static str, ty: &'static str) -> AppFieldDescriptor {
    field(name, W::Struct, &[], ty, ty, J::Ref(ty), true)
}
const fn id(name: &'static str) -> AppFieldDescriptor {
    field(
        name,
        W::Identifier,
        &[B::NonZero],
        "ControlOperationId",
        "ControlOperationId",
        J::Identifier,
        true,
    )
}
const fn revision(name: &'static str, required: bool) -> AppFieldDescriptor {
    field(name, W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, required)
}
const fn version(name: &'static str, required: bool) -> AppFieldDescriptor {
    field(
        name,
        W::Struct,
        &[],
        "WorkbenchCheckpointVersion",
        "WorkbenchCheckpointAbsentVersion | WorkbenchCheckpointPresentVersion",
        J::OneOfRef(VERSION_TYPES),
        required,
    )
}
const fn path(name: &'static str) -> AppFieldDescriptor {
    field(name, W::Utf8, &[B::WorkbenchFilePathBytes], "String", "string", J::String, true)
}
const fn strings(name: &'static str, restore: bool) -> AppFieldDescriptor {
    field(
        name,
        W::Sequence,
        if restore {
            &[B::WorkbenchCheckpointPaths, B::WorkbenchRestoreTextBytes]
        } else {
            &[B::WorkbenchCheckpointPaths, B::WorkbenchCheckpointTextBytes]
        },
        "Vec<String>",
        "readonly string[]",
        J::StringArray,
        true,
    )
}
const fn kind(value: &'static str, values: &'static [&'static str]) -> AppFieldDescriptor {
    field("kind", W::U16, &[], "WorkbenchIntent", value, J::Enum(values), true)
}

pub(super) const CHECKPOINT_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "WorkbenchCheckpointAbsentVersion",
        rust_type: "WorkbenchCheckpointVersion",
        fields: &[field(
            "kind",
            W::U16,
            &[],
            "WorkbenchCheckpointVersion",
            "\"absent\"",
            J::Enum(&["absent"]),
            true,
        )],
    },
    AppTypeDescriptor {
        name: "WorkbenchCheckpointPresentVersion",
        rust_type: "WorkbenchCheckpointVersion",
        fields: &[
            field(
                "kind",
                W::U16,
                &[],
                "WorkbenchCheckpointVersion",
                "\"present\"",
                J::Enum(&["present"]),
                true,
            ),
            field("digest", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
            field("bytes", W::U64, &[], "u64", "UInt64", J::U64String, true),
            field(
                "mode",
                W::U16,
                &[],
                "WorkbenchCheckpointFileMode",
                "\"regular\" | \"executable\"",
                J::Enum(&["regular", "executable"]),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchCheckpointReferences",
        rust_type: "WorkbenchCheckpointReferences",
        fields: &[
            revision("sourceConversationRevision", true),
            field("contextGeneration", W::U64, &[], "u64", "UInt64", J::U64String, true),
            field("briefRevision", W::U64, &[], "u64", "UInt64", J::U64String, true),
            field("hasGoalRevision", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            revision("goalRevision", false),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchCheckpointPath",
        rust_type: "WorkbenchCheckpointPath",
        fields: &[
            path("path"),
            version("checkpoint", true),
            field("hasExpectedCurrent", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            version("expectedCurrent", false),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchCheckpointReceipt",
        rust_type: "WorkbenchCheckpointReceipt",
        fields: &[
            id("checkpoint"),
            nested("query", "WorkbenchQuery"),
            revision("acceptedRevision", true),
            field(
                "name",
                W::Utf8,
                &[B::WorkbenchCheckpointNameBytes],
                "WorkbenchCheckpointName",
                "string",
                J::String,
                true,
            ),
            nested("references", "WorkbenchCheckpointReferences"),
            field(
                "paths",
                W::Sequence,
                &[B::WorkbenchCheckpointPaths, B::SortedUnique],
                "Vec<WorkbenchCheckpointPath>",
                "readonly WorkbenchCheckpointPath[]",
                J::ArrayRef("WorkbenchCheckpointPath"),
                true,
            ),
            strings("exclusions", false),
            strings("externalEffects", false),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchRewindRequest",
        rust_type: "WorkbenchRewindRequest",
        fields: &[
            nested("query", "WorkbenchQuery"),
            revision("revision", true),
            id("checkpoint"),
            field(
                "mode",
                W::U16,
                &[],
                "WorkbenchRewindMode",
                "\"files_only\" | \"conversation_only\" | \"combined\"",
                J::Enum(&["files_only", "conversation_only", "combined"]),
                true,
            ),
            field(
                "child",
                W::Identifier,
                &[B::NonZero],
                "Option<ConversationId>",
                "ConversationId",
                J::Identifier,
                false,
            ),
            field(
                "allocation",
                W::Option,
                &[],
                "Option<WorkbenchForkBudget>",
                "WorkbenchForkBudget",
                J::Ref("WorkbenchForkBudget"),
                false,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchRewindPath",
        rust_type: "WorkbenchRewindPath",
        fields: &[
            path("path"),
            version("checkpoint", true),
            field("hasExpectedCurrent", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            version("expectedCurrent", false),
            version("observedCurrent", true),
            field(
                "disposition",
                W::U16,
                &[],
                "WorkbenchRewindDisposition",
                "\"restore\" | \"unchanged\" | \"conflict\" | \"unsealed\"",
                J::Enum(&["restore", "unchanged", "conflict", "unsealed"]),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchRewindPreview",
        rust_type: "WorkbenchRewindPreview",
        fields: &[
            nested("request", "WorkbenchRewindRequest"),
            field("previewDigest", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
            field(
                "paths",
                W::Sequence,
                &[B::WorkbenchCheckpointPaths, B::SortedUnique],
                "Vec<WorkbenchRewindPath>",
                "readonly WorkbenchRewindPath[]",
                J::ArrayRef("WorkbenchRewindPath"),
                true,
            ),
            strings("exclusions", false),
            strings("externalEffects", false),
            field(
                "conversationHistoryPreserved",
                W::Boolean,
                &[],
                "bool",
                "boolean",
                J::Boolean,
                true,
            ),
            field("accountingPreserved", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchCreateCheckpointIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            kind("\"createCheckpoint\"", &["createCheckpoint"]),
            field(
                "name",
                W::Utf8,
                &[B::WorkbenchCheckpointNameBytes],
                "WorkbenchCheckpointName",
                "string",
                J::String,
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchApplyRewindIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            kind("\"applyRewind\"", &["applyRewind"]),
            nested("preview", "WorkbenchRewindPreview"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchRestoreReceipt",
        rust_type: "WorkbenchRestoreReceipt",
        fields: &[
            id("restore"),
            id("checkpoint"),
            id("recoveryCheckpoint"),
            nested("query", "WorkbenchQuery"),
            revision("acceptedRevision", true),
            field(
                "status",
                W::U16,
                &[],
                "WorkbenchRestoreStatus",
                "\"applied\" | \"conflict\" | \"recoveryRequired\"",
                J::Enum(&["applied", "conflict", "recoveryRequired"]),
                true,
            ),
            strings("restored", true),
            strings("conflicts", true),
            strings("externalEffects", true),
        ],
    },
];

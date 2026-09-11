//! Deterministic local source-handle preview fields in canonical wire order.

use super::super::{
    AppFieldDescriptor, AppTypeDescriptor, CanonicalWireType as W, FieldBound as B, JsonShape as J,
    field,
};

const fn nested(name: &'static str, ty: &'static str) -> AppFieldDescriptor {
    field(name, W::Struct, &[], ty, ty, J::Ref(ty), true)
}
const fn bytes(name: &'static str) -> AppFieldDescriptor {
    field(name, W::U64, &[], "u64", "UInt64", J::U64String, true)
}
const fn count(name: &'static str) -> AppFieldDescriptor {
    field(name, W::U32, &[B::WorkbenchCompactionEntries], "u32", "number", J::U32, true)
}

pub(super) const COMPACTION_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "WorkbenchCompactionRequest",
        rust_type: "WorkbenchCompactionRequest",
        fields: &[
            nested("query", "WorkbenchQuery"),
            field("revision", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true),
            field("hasFocus", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            field(
                "focus",
                W::Utf8,
                &[B::WorkbenchCompactionFocusBytes],
                "WorkbenchCompactionFocus",
                "string",
                J::String,
                false,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchCompactionEntry",
        rust_type: "WorkbenchCompactionEntry",
        fields: &[
            field(
                "invocation",
                W::Identifier,
                &[B::NonZero],
                "WorkbenchInvocationId",
                "WorkbenchInvocationId",
                J::Identifier,
                true,
            ),
            field("sourceDigest", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
            bytes("sourceBytes"),
            field(
                "replacementDigest",
                W::Digest,
                &[],
                "Sha256Digest",
                "Sha256Digest",
                J::Digest,
                true,
            ),
            bytes("replacementBytes"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchCompactionPreview",
        rust_type: "WorkbenchCompactionPreview",
        fields: &[
            nested("request", "WorkbenchCompactionRequest"),
            field("generation", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true),
            count("recentPreserved"),
            count("pinnedPreserved"),
            count("unresolvedPreserved"),
            count("unsavablePreserved"),
            field(
                "entries",
                W::Sequence,
                &[B::WorkbenchCompactionEntries, B::SortedUnique],
                "Vec<WorkbenchCompactionEntry>",
                "readonly WorkbenchCompactionEntry[]",
                J::ArrayRef("WorkbenchCompactionEntry"),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchApplyCompactionIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            field(
                "kind",
                W::U16,
                &[],
                "WorkbenchIntent",
                "\"applyCompaction\"",
                J::Enum(&["applyCompaction"]),
                true,
            ),
            nested("preview", "WorkbenchCompactionPreview"),
        ],
    },
];

//! Paged harness-improvement metadata and independently addressed retained text.

use super::super::{
    AppFieldDescriptor, AppTypeDescriptor, CanonicalWireType as W, FieldBound as B, JsonShape as J,
    field,
};

const fn id(name: &'static str, rust_type: &'static str) -> AppFieldDescriptor {
    field(name, W::Identifier, &[B::NonZero], rust_type, rust_type, J::Identifier, true)
}

const fn digest(name: &'static str) -> AppFieldDescriptor {
    field(name, W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true)
}

const fn nested(name: &'static str, ty: &'static str) -> AppFieldDescriptor {
    field(name, W::Struct, &[], ty, ty, J::Ref(ty), true)
}

const fn revision() -> AppFieldDescriptor {
    field("revision", W::U64, &[], "u64", "UInt64", J::U64String, true)
}

pub(super) const IMPROVEMENT_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "ImprovementEvaluation",
        rust_type: "ImprovementEvaluation",
        fields: &[
            id("conversation", "ConversationId"),
            id("run", "RunId"),
            id("target", "WorkspaceId"),
        ],
    },
    AppTypeDescriptor {
        name: "ImprovementTextReference",
        rust_type: "ImprovementTextReference",
        fields: &[
            digest("digest"),
            field("bytes", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true),
        ],
    },
    AppTypeDescriptor {
        name: "ImprovementPageCursor",
        rust_type: "ImprovementPageCursor",
        fields: &[
            id("workspace", "WorkspaceId"),
            field(
                "candidate",
                W::Option,
                &[],
                "Option<Sha256Digest>",
                "Sha256Digest",
                J::Digest,
                false,
            ),
            field("revision", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true),
            field("highwaterSequence", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true),
            field("afterSequence", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true),
        ],
    },
    AppTypeDescriptor {
        name: "ImprovementCandidateSummary",
        rust_type: "ImprovementCandidateSummary",
        fields: &[
            digest("id"),
            nested("proposal", "ImprovementTextReference"),
            field("evidenceCount", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true),
            field(
                "evaluation",
                W::Option,
                &[],
                "Option<ImprovementEvaluation>",
                "ImprovementEvaluation",
                J::Ref("ImprovementEvaluation"),
                false,
            ),
            field("dismissed", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
        ],
    },
    AppTypeDescriptor {
        name: "ImprovementEvidenceSummary",
        rust_type: "ImprovementEvidenceSummary",
        fields: &[id("run", "RunId"), nested("summary", "ImprovementTextReference")],
    },
    AppTypeDescriptor {
        name: "ImprovementPage",
        rust_type: "ImprovementPage",
        fields: &[
            id("workspace", "WorkspaceId"),
            revision(),
            field(
                "candidates",
                W::Sequence,
                &[B::ImprovementPage],
                "Vec<ImprovementCandidateSummary>",
                "readonly ImprovementCandidateSummary[]",
                J::ArrayRef("ImprovementCandidateSummary"),
                true,
            ),
            field(
                "next",
                W::Option,
                &[],
                "Option<ImprovementPageCursor>",
                "ImprovementPageCursor",
                J::Ref("ImprovementPageCursor"),
                false,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "ImprovementEvidencePage",
        rust_type: "ImprovementEvidencePage",
        fields: &[
            id("workspace", "WorkspaceId"),
            digest("candidate"),
            revision(),
            field(
                "evidence",
                W::Sequence,
                &[B::ImprovementPage],
                "Vec<ImprovementEvidenceSummary>",
                "readonly ImprovementEvidenceSummary[]",
                J::ArrayRef("ImprovementEvidenceSummary"),
                true,
            ),
            field(
                "next",
                W::Option,
                &[],
                "Option<ImprovementPageCursor>",
                "ImprovementPageCursor",
                J::Ref("ImprovementPageCursor"),
                false,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "ImprovementTextQuery",
        rust_type: "ImprovementTextQuery",
        fields: &[
            id("workspace", "WorkspaceId"),
            digest("candidate"),
            field("run", W::Option, &[], "Option<RunId>", "RunId", J::Identifier, false),
            nested("source", "ImprovementTextReference"),
            field("offset", W::U64, &[], "u64", "UInt64", J::U64String, true),
        ],
    },
    AppTypeDescriptor {
        name: "ImprovementTextPage",
        rust_type: "ImprovementTextPage",
        fields: &[
            nested("query", "ImprovementTextQuery"),
            field(
                "text",
                W::Utf8,
                &[B::ImprovementTextChunkBytes],
                "String",
                "string",
                J::String,
                true,
            ),
            field("next", W::Option, &[], "Option<u64>", "UInt64", J::U64String, false),
        ],
    },
];

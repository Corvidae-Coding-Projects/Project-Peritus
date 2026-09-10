//! Structured-review mutation descriptors and their content-bound anchors.

use super::super::{
    AppFieldDescriptor, AppTypeDescriptor, CanonicalWireType as W, FieldBound as B, JsonShape as J,
    field,
};

const fn id(name: &'static str, ty: &'static str) -> AppFieldDescriptor {
    field(name, W::Identifier, &[B::NonZero], ty, ty, J::Identifier, true)
}

const fn digest(name: &'static str) -> AppFieldDescriptor {
    field(name, W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true)
}

const fn intent_kind(
    typescript_type: &'static str,
    values: &'static [&'static str],
) -> AppFieldDescriptor {
    field("kind", W::U16, &[], "WorkbenchIntent", typescript_type, J::Enum(values), true)
}

pub(super) const REVIEW_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "WorkbenchReviewRange",
        rust_type: "WorkbenchReviewRange",
        fields: &[
            field("oldStart", W::U32, &[], "u32", "number", J::U32, true),
            field("oldLines", W::U32, &[], "u32", "number", J::U32, true),
            field("newStart", W::U32, &[], "u32", "number", J::U32, true),
            field("newLines", W::U32, &[], "u32", "number", J::U32, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchReviewAnchor",
        rust_type: "WorkbenchReviewAnchor",
        fields: &[
            id("run", "RunId"),
            id("workspace", "WorkspaceId"),
            digest("candidateDigest"),
            digest("diffDigest"),
            digest("beforeBlobDigest"),
            digest("afterBlobDigest"),
            digest("contextDigest"),
            field(
                "path",
                W::Utf8,
                &[B::WorkbenchFilePathBytes],
                "String",
                "string",
                J::String,
                true,
            ),
            field(
                "target",
                W::U16,
                &[],
                "WorkbenchReviewTarget",
                "\"file\" | \"hunk\"",
                J::Enum(&["file", "hunk"]),
                true,
            ),
            field(
                "range",
                W::Struct,
                &[],
                "WorkbenchReviewRange",
                "WorkbenchReviewRange",
                J::Ref("WorkbenchReviewRange"),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchAddReviewIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            intent_kind("\"addReview\"", &["addReview"]),
            field(
                "anchor",
                W::Struct,
                &[],
                "WorkbenchReviewAnchor",
                "WorkbenchReviewAnchor",
                J::Ref("WorkbenchReviewAnchor"),
                true,
            ),
            field(
                "feedback",
                W::U16,
                &[],
                "WorkbenchReviewFeedback",
                "\"explain\" | \"requestRevision\" | \"keepBehavior\" | \"leaveAlone\"",
                J::Enum(&["explain", "requestRevision", "keepBehavior", "leaveAlone"]),
                true,
            ),
            field(
                "message",
                W::Utf8,
                &[B::WorkbenchInputBytes],
                "WorkbenchInputText",
                "string",
                J::String,
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchRebindReviewIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            intent_kind("\"rebindReview\"", &["rebindReview"]),
            id("comment", "ControlOperationId"),
            field(
                "anchor",
                W::Struct,
                &[],
                "WorkbenchReviewAnchor",
                "WorkbenchReviewAnchor",
                J::Ref("WorkbenchReviewAnchor"),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchDismissReviewIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            intent_kind("\"dismissReview\"", &["dismissReview"]),
            id("comment", "ControlOperationId"),
        ],
    },
];

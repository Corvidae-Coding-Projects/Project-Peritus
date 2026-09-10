//! Context metadata in exact canonical field order. Sizes never imply token accounting.

use super::super::{
    AppFieldDescriptor, AppTypeDescriptor, CanonicalWireType as W, FieldBound as B, JsonShape as J,
    field,
};

const fn invocation(name: &'static str) -> AppFieldDescriptor {
    field(
        name,
        W::Identifier,
        &[B::NonZero],
        "WorkbenchInvocationId",
        "WorkbenchInvocationId",
        J::Identifier,
        true,
    )
}
const fn digest(name: &'static str) -> AppFieldDescriptor {
    field(name, W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true)
}
const fn nested(name: &'static str, ty: &'static str) -> AppFieldDescriptor {
    field(name, W::Struct, &[], ty, ty, J::Ref(ty), true)
}
const fn tag(
    rust: &'static str,
    ts: &'static str,
    values: &'static [&'static str],
) -> AppFieldDescriptor {
    field("kind", W::U16, &[], rust, ts, J::Enum(values), true)
}

pub(super) const CONTEXT_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "WorkbenchContextNextView",
        rust_type: "WorkbenchContextView",
        fields: &[tag("WorkbenchContextView", "\"next\" | \"history\"", &["next", "history"])],
    },
    AppTypeDescriptor {
        name: "WorkbenchContextInvocationView",
        rust_type: "WorkbenchContextView",
        fields: &[
            tag("WorkbenchContextView", "\"invocation\"", &["invocation"]),
            invocation("invocation"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchContextQuery",
        rust_type: "WorkbenchContextQuery",
        fields: &[
            nested("query", "WorkbenchQuery"),
            field("revision", W::U64, &[], "u64", "UInt64", J::U64String, true),
            field("offset", W::U32, &[B::WorkbenchContextRows], "u32", "number", J::U32, true),
            field(
                "view",
                W::Struct,
                &[],
                "WorkbenchContextView",
                "WorkbenchContextNextView | WorkbenchContextInvocationView",
                J::OneOfRef(&["WorkbenchContextNextView", "WorkbenchContextInvocationView"]),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchContextInputSource",
        rust_type: "WorkbenchContextSource",
        fields: &[
            tag("WorkbenchContextSource", "\"input\"", &["input"]),
            nested("selected", "WorkbenchInputSelection"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchContextReplySource",
        rust_type: "WorkbenchContextSource",
        fields: &[
            tag("WorkbenchContextSource", "\"publicReply\"", &["publicReply"]),
            invocation("invocation"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchContextMessageSource",
        rust_type: "WorkbenchContextSource",
        fields: &[
            tag("WorkbenchContextSource", "\"message\"", &["message"]),
            field("ordinal", W::U32, &[], "u32", "number", J::U32, true),
            field(
                "role",
                W::U16,
                &[],
                "WorkbenchMessageRole",
                "\"system\" | \"developer\" | \"user\" | \"assistant\" | \"tool\"",
                J::Enum(&["system", "developer", "user", "assistant", "tool"]),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchContextInvocationSource",
        rust_type: "WorkbenchContextSource",
        fields: &[
            tag("WorkbenchContextSource", "\"invocation\"", &["invocation"]),
            invocation("invocation"),
            digest("manifestDigest"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchContextRow",
        rust_type: "WorkbenchContextRow",
        fields: &[
            field(
                "source",
                W::Struct,
                &[],
                "WorkbenchContextSource",
                "WorkbenchContextInputSource | WorkbenchContextReplySource | WorkbenchContextMessageSource | WorkbenchContextInvocationSource | WorkbenchContextImageSource | WorkbenchContextFileSource",
                J::OneOfRef(&[
                    "WorkbenchContextInputSource",
                    "WorkbenchContextReplySource",
                    "WorkbenchContextMessageSource",
                    "WorkbenchContextInvocationSource",
                    "WorkbenchContextImageSource",
                    "WorkbenchContextFileSource",
                ]),
                true,
            ),
            digest("digest"),
            field(
                "bytes",
                W::U64,
                &[B::WorkbenchContextSourceBytes],
                "u64",
                "UInt64",
                J::U64String,
                true,
            ),
            field(
                "disposition",
                W::U16,
                &[],
                "WorkbenchContextDisposition",
                "\"eligible\" | \"included\" | \"held\" | \"withdrawn\" | \"superseded\" | \"dependencyBlocked\" | \"awaitingLaterInput\" | \"deselected\" | \"userExcluded\"",
                J::Enum(&[
                    "eligible",
                    "included",
                    "held",
                    "withdrawn",
                    "superseded",
                    "dependencyBlocked",
                    "awaitingLaterInput",
                    "deselected",
                    "userExcluded",
                ]),
                true,
            ),
            field("hasPreference", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            field(
                "preference",
                W::U16,
                &[],
                "WorkbenchContextPreference",
                "\"pinned\" | \"excluded\"",
                J::Enum(&["pinned", "excluded"]),
                false,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchSetContextIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            tag("WorkbenchIntent", "\"setContext\"", &["setContext"]),
            field(
                "source",
                W::Struct,
                &[],
                "WorkbenchContextSource",
                "WorkbenchContextInputSource | WorkbenchContextReplySource | WorkbenchContextImageSource | WorkbenchContextFileSource",
                J::OneOfRef(&[
                    "WorkbenchContextInputSource",
                    "WorkbenchContextReplySource",
                    "WorkbenchContextImageSource",
                    "WorkbenchContextFileSource",
                ]),
                true,
            ),
            field("hasPreference", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            field(
                "preference",
                W::U16,
                &[],
                "WorkbenchContextPreference",
                "\"pinned\" | \"excluded\"",
                J::Enum(&["pinned", "excluded"]),
                false,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchContextImageSource",
        rust_type: "WorkbenchContextSource",
        fields: &[
            tag("WorkbenchContextSource", "\"image\"", &["image"]),
            field(
                "operation",
                W::Identifier,
                &[B::NonZero],
                "ControlOperationId",
                "ControlOperationId",
                J::Identifier,
                true,
            ),
            field(
                "input",
                W::Identifier,
                &[B::NonZero],
                "WorkbenchInputId",
                "WorkbenchInputId",
                J::Identifier,
                true,
            ),
            field(
                "artifact",
                W::Identifier,
                &[B::NonZero],
                "ArtifactId",
                "ArtifactId",
                J::Identifier,
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchContextSeal",
        rust_type: "WorkbenchContextSeal",
        fields: &[
            invocation("invocation"),
            digest("requestDigest"),
            digest("manifestDigest"),
            field("generation", W::U64, &[], "u64", "UInt64", J::U64String, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchContextPage",
        rust_type: "WorkbenchContextPage",
        fields: &[
            nested("query", "WorkbenchContextQuery"),
            field("total", W::U32, &[B::WorkbenchContextRows], "u32", "number", J::U32, true),
            field("hasSeal", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            field(
                "seal",
                W::Struct,
                &[],
                "WorkbenchContextSeal",
                "WorkbenchContextSeal",
                J::Ref("WorkbenchContextSeal"),
                false,
            ),
            field(
                "rows",
                W::Sequence,
                &[B::WorkbenchContextPage],
                "Vec<WorkbenchContextRow>",
                "readonly WorkbenchContextRow[]",
                J::ArrayRef("WorkbenchContextRow"),
                true,
            ),
        ],
    },
];

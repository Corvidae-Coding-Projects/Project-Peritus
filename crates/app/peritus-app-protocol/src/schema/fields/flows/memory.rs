//! Project-guidance DTO metadata in exact canonical field order.

use super::super::{
    AppFieldDescriptor, AppTypeDescriptor, CanonicalWireType as W, FieldBound as B, JsonShape as J,
    field,
};

const fn identifier(name: &'static str, rust: &'static str) -> AppFieldDescriptor {
    field(name, W::Identifier, &[B::NonZero], rust, rust, J::Identifier, true)
}

const fn u64_field(name: &'static str, nonzero: bool) -> AppFieldDescriptor {
    field(
        name,
        W::U64,
        if nonzero { &[B::NonZero] } else { &[] },
        "u64",
        "UInt64",
        J::U64String,
        true,
    )
}

const fn nested(name: &'static str, ty: &'static str) -> AppFieldDescriptor {
    field(name, W::Struct, &[], ty, ty, J::Ref(ty), true)
}

const fn kind(
    rust: &'static str,
    ts: &'static str,
    values: &'static [&'static str],
) -> AppFieldDescriptor {
    field("kind", W::U16, &[], rust, ts, J::Enum(values), true)
}

const SCOPE: AppFieldDescriptor = field(
    "scope",
    W::Struct,
    &[],
    "WorkbenchGuidanceScope",
    "WorkbenchGuidanceProjectScope | WorkbenchGuidanceConversationScope",
    J::OneOfRef(&["WorkbenchGuidanceProjectScope", "WorkbenchGuidanceConversationScope"]),
    true,
);

const CONTENT: AppFieldDescriptor = field(
    "content",
    W::Struct,
    &[],
    "WorkbenchGuidanceContent",
    "WorkbenchGuidanceContent",
    J::Ref("WorkbenchGuidanceContent"),
    true,
);

const SELECTION: AppFieldDescriptor = field(
    "selection",
    W::Struct,
    &[],
    "WorkbenchGuidanceSelection",
    "WorkbenchGuidanceSelection",
    J::Ref("WorkbenchGuidanceSelection"),
    true,
);

const DEPENDENCY: AppFieldDescriptor =
    field("expectedDependencyRevision", W::U64, &[], "u64", "UInt64", J::U64String, true);

pub(super) const MEMORY_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "WorkbenchGuidanceProjectScope",
        rust_type: "WorkbenchGuidanceScope",
        fields: &[kind("WorkbenchGuidanceScope", "\"project\"", &["project"])],
    },
    AppTypeDescriptor {
        name: "WorkbenchGuidanceConversationScope",
        rust_type: "WorkbenchGuidanceScope",
        fields: &[
            kind("WorkbenchGuidanceScope", "\"conversation\"", &["conversation"]),
            identifier("conversation", "ConversationId"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchGuidanceUserSource",
        rust_type: "WorkbenchGuidanceSource",
        fields: &[kind("WorkbenchGuidanceSource", "\"userAuthored\"", &["userAuthored"])],
    },
    AppTypeDescriptor {
        name: "WorkbenchGuidanceReplySource",
        rust_type: "WorkbenchGuidanceSource",
        fields: &[
            kind("WorkbenchGuidanceSource", "\"acceptedPublicReply\"", &["acceptedPublicReply"]),
            identifier("operation", "ControlOperationId"),
            identifier("invocation", "WorkbenchInvocationId"),
            field("digest", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchGuidanceContent",
        rust_type: "WorkbenchGuidanceContent",
        fields: &[
            field(
                "text",
                W::Utf8,
                &[B::CodecStringBytes],
                "WorkbenchGuidanceText",
                "string",
                J::String,
                true,
            ),
            field(
                "source",
                W::Struct,
                &[],
                "WorkbenchGuidanceSource",
                "WorkbenchGuidanceUserSource | WorkbenchGuidanceReplySource",
                J::OneOfRef(&["WorkbenchGuidanceUserSource", "WorkbenchGuidanceReplySource"]),
                true,
            ),
            SCOPE,
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchGuidanceIdentity",
        rust_type: "WorkbenchGuidanceIdentity",
        fields: &[identifier("id", "ControlOperationId"), identifier("workspace", "WorkspaceId")],
    },
    AppTypeDescriptor {
        name: "WorkbenchGuidanceVersion",
        rust_type: "WorkbenchGuidanceVersion",
        fields: &[u64_field("record", true), u64_field("dependency", true)],
    },
    AppTypeDescriptor {
        name: "WorkbenchGuidanceValidation",
        rust_type: "WorkbenchGuidanceValidation",
        fields: &[
            identifier("operation", "ControlOperationId"),
            field("contentDigest", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchGuidanceRecord",
        rust_type: "WorkbenchGuidanceRecord",
        fields: &[
            nested("identity", "WorkbenchGuidanceIdentity"),
            nested("version", "WorkbenchGuidanceVersion"),
            CONTENT,
            field("pinned", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            nested("lastValidation", "WorkbenchGuidanceValidation"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchGuidancePrior",
        rust_type: "WorkbenchGuidancePrior",
        fields: &[
            u64_field("revision", true),
            field("digest", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
            SCOPE,
            field("pinned", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchGuidanceTombstone",
        rust_type: "WorkbenchGuidanceTombstone",
        fields: &[
            nested("identity", "WorkbenchGuidanceIdentity"),
            nested("prior", "WorkbenchGuidancePrior"),
            identifier("forgottenBy", "ControlOperationId"),
            field(
                "reason",
                W::Utf8,
                &[B::CodecStringBytes],
                "WorkbenchGuidanceReason",
                "string",
                J::String,
                true,
            ),
            u64_field("dependencyRevision", true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchGuidanceSelection",
        rust_type: "WorkbenchGuidanceSelection",
        fields: &[identifier("id", "ControlOperationId"), u64_field("expectedRevision", true)],
    },
    AppTypeDescriptor {
        name: "WorkbenchSaveGuidanceIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            kind("WorkbenchIntent", "\"saveGuidance\"", &["saveGuidance"]),
            DEPENDENCY,
            CONTENT,
            field("pinned", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchReviseGuidanceIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            kind("WorkbenchIntent", "\"reviseGuidance\"", &["reviseGuidance"]),
            SELECTION,
            DEPENDENCY,
            CONTENT,
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchPinGuidanceIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            kind("WorkbenchIntent", "\"pinGuidance\"", &["pinGuidance"]),
            SELECTION,
            DEPENDENCY,
            field("pinned", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchScopeGuidanceIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            kind("WorkbenchIntent", "\"scopeGuidance\"", &["scopeGuidance"]),
            SELECTION,
            DEPENDENCY,
            SCOPE,
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchForgetGuidanceIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            kind("WorkbenchIntent", "\"forgetGuidance\"", &["forgetGuidance"]),
            SELECTION,
            DEPENDENCY,
            field(
                "reason",
                W::Utf8,
                &[B::CodecStringBytes],
                "WorkbenchGuidanceReason",
                "string",
                J::String,
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchMemoryQuery",
        rust_type: "WorkbenchMemoryQuery",
        fields: &[
            nested("query", "WorkbenchQuery"),
            u64_field("dependencyRevision", false),
            field("offset", W::U32, &[], "u32", "number", J::U32, true),
            field("includeForgotten", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchMemoryActiveRow",
        rust_type: "WorkbenchMemoryRow",
        fields: &[
            kind("WorkbenchMemoryRow", "\"active\"", &["active"]),
            nested("record", "WorkbenchGuidanceRecord"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchMemoryForgottenRow",
        rust_type: "WorkbenchMemoryRow",
        fields: &[
            kind("WorkbenchMemoryRow", "\"forgotten\"", &["forgotten"]),
            nested("tombstone", "WorkbenchGuidanceTombstone"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchMemory",
        rust_type: "WorkbenchMemory",
        fields: &[
            nested("query", "WorkbenchMemoryQuery"),
            u64_field("dependencyRevision", false),
            field("total", W::U32, &[], "u32", "number", J::U32, true),
            field(
                "rows",
                W::Sequence,
                &[B::CodecCollectionItems],
                "Vec<WorkbenchMemoryRow>",
                "readonly (WorkbenchMemoryActiveRow | WorkbenchMemoryForgottenRow)[]",
                J::OneOfArrayRef(&["WorkbenchMemoryActiveRow", "WorkbenchMemoryForgottenRow"]),
                true,
            ),
        ],
    },
];

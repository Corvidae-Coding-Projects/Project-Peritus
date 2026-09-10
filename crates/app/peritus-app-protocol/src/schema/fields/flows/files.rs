//! File preview, range and retained-version wire descriptors.
use super::super::{
    AppFieldDescriptor, AppTypeDescriptor, CanonicalWireType as W, FieldBound as B, JsonShape as J,
    field,
};
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
const fn number(name: &'static str) -> AppFieldDescriptor {
    field(name, W::U64, &[], "u64", "UInt64", J::U64String, true)
}
const fn kind(
    value: &'static str,
    ts: &'static str,
    values: &'static [&'static str],
) -> AppFieldDescriptor {
    field("kind", W::U16, &[], value, ts, J::Enum(values), true)
}
const fn mode() -> AppFieldDescriptor {
    field(
        "mode",
        W::U16,
        &[],
        "WorkbenchFileMode",
        "\"snapshot\" | \"refreshOnRequest\"",
        J::Enum(&["snapshot", "refreshOnRequest"]),
        true,
    )
}
const fn label(name: &'static str) -> AppFieldDescriptor {
    field(name, W::Utf8, &[B::WorkbenchFilePathBytes], "String", "string", J::String, true)
}
pub(super) const FILE_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "WorkbenchFileUpload",
        rust_type: "WorkbenchFileUpload",
        fields: &[
            nested("query", "WorkbenchQuery"),
            field("revision", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true),
            nested("metadata", "ArtifactMetadata"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchFileRangeAll",
        rust_type: "WorkbenchFileRange",
        fields: &[kind("WorkbenchFileRange", "\"all\"", &["all"])],
    },
    AppTypeDescriptor {
        name: "WorkbenchFileRangeBytes",
        rust_type: "WorkbenchFileRange",
        fields: &[
            kind("WorkbenchFileRange", "\"bytes\"", &["bytes"]),
            number("start"),
            number("end"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchFileRangeLines",
        rust_type: "WorkbenchFileRange",
        fields: &[
            kind("WorkbenchFileRange", "\"lines\"", &["lines"]),
            field("first", W::U32, &[B::NonZero], "u32", "number", J::U32, true),
            field("last", W::U32, &[B::NonZero], "u32", "number", J::U32, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchFileRequest",
        rust_type: "WorkbenchFileRequest",
        fields: &[
            nested("query", "WorkbenchQuery"),
            field("revision", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true),
            label("path"),
            field(
                "range",
                W::Struct,
                &[],
                "WorkbenchFileRange",
                "WorkbenchFileRangeAll | WorkbenchFileRangeBytes | WorkbenchFileRangeLines",
                J::OneOfRef(&[
                    "WorkbenchFileRangeAll",
                    "WorkbenchFileRangeBytes",
                    "WorkbenchFileRangeLines",
                ]),
                true,
            ),
            mode(),
            field(
                "provider",
                W::Identifier,
                &[B::NonZero],
                "ProviderProfileId",
                "ProviderProfileId",
                J::Identifier,
                true,
            ),
            nested("model", "WorkbenchImageModel"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchFileMetadata",
        rust_type: "WorkbenchFileMetadata",
        fields: &[
            field("sourceDigest", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
            field(
                "sourceBytes",
                W::U64,
                &[B::WorkbenchContextSourceBytes],
                "u64",
                "UInt64",
                J::U64String,
                true,
            ),
            number("rangeStart"),
            number("rangeEnd"),
            field("digest", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchFilePreview",
        rust_type: "WorkbenchFilePreview",
        fields: &[
            nested("request", "WorkbenchFileRequest"),
            field("folder", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
            nested("file", "WorkbenchFileMetadata"),
            field("providerRevision", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true),
            field(
                "resolvedModel",
                W::Utf8,
                &[B::ProductModelBytes],
                "String",
                "string",
                J::String,
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchFileImportRequest",
        rust_type: "WorkbenchFileImportRequest",
        fields: &[
            nested("selection", "WorkbenchFileRequest"),
            field(
                "artifact",
                W::Identifier,
                &[B::NonZero],
                "ArtifactId",
                "ArtifactId",
                J::Identifier,
                true,
            ),
            nested("file", "WorkbenchFileMetadata"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchFileImportPreview",
        rust_type: "WorkbenchFileImportPreview",
        fields: &[
            nested("request", "WorkbenchFileImportRequest"),
            field("providerRevision", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true),
            field(
                "resolvedModel",
                W::Utf8,
                &[B::ProductModelBytes],
                "String",
                "string",
                J::String,
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchFileQuery",
        rust_type: "WorkbenchFileQuery",
        fields: &[
            nested("query", "WorkbenchQuery"),
            field("revision", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true),
            field("offset", W::U32, &[B::WorkbenchFileHistory], "u32", "number", J::U32, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchFileRow",
        rust_type: "WorkbenchFileRow",
        fields: &[
            id("attachment"),
            id("version"),
            label("label"),
            mode(),
            nested("file", "WorkbenchFileMetadata"),
            field("selected", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            field("eligible", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchFilePage",
        rust_type: "WorkbenchFilePage",
        fields: &[
            nested("query", "WorkbenchFileQuery"),
            field("total", W::U32, &[B::WorkbenchFileHistory], "u32", "number", J::U32, true),
            field(
                "rows",
                W::Sequence,
                &[B::WorkbenchFilePage],
                "Vec<WorkbenchFileRow>",
                "readonly WorkbenchFileRow[]",
                J::ArrayRef("WorkbenchFileRow"),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchAttachFileIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            kind("WorkbenchIntent", "\"attachFile\"", &["attachFile"]),
            nested("preview", "WorkbenchFilePreview"),
            field(
                "text",
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
        name: "WorkbenchSelectFileIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            kind("WorkbenchIntent", "\"selectFile\"", &["selectFile"]),
            id("attachment"),
            field("selected", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchAttachFileImportIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            kind("WorkbenchIntent", "\"attachFileImport\"", &["attachFileImport"]),
            nested("preview", "WorkbenchFileImportPreview"),
            field(
                "text",
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
        name: "WorkbenchContextFileSource",
        rust_type: "WorkbenchContextSource",
        fields: &[
            kind("WorkbenchContextSource", "\"file\"", &["file"]),
            id("attachment"),
            id("version"),
        ],
    },
];

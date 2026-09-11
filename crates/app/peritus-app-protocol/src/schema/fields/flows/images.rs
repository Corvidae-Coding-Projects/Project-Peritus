//! Explicit-image fields in exact binary order, including mandatory preview model effort.

use super::super::{
    AppFieldDescriptor, AppTypeDescriptor, CanonicalWireType as W, FieldBound as B, JsonShape as J,
    field,
};
const fn nested(name: &'static str, ty: &'static str) -> AppFieldDescriptor {
    field(name, W::Struct, &[], ty, ty, J::Ref(ty), true)
}
const fn id(name: &'static str, ty: &'static str) -> AppFieldDescriptor {
    field(name, W::Identifier, &[B::NonZero], ty, ty, J::Identifier, true)
}
const fn revision(name: &'static str) -> AppFieldDescriptor {
    field(name, W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true)
}

pub(super) const IMAGE_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "WorkbenchImageUpload",
        rust_type: "WorkbenchImageUpload",
        fields: &[
            nested("query", "WorkbenchQuery"),
            revision("revision"),
            nested("metadata", "ArtifactMetadata"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchImageModel",
        rust_type: "ProductModelChoice",
        fields: &[
            field("id", W::Utf8, &[B::ProductModelBytes], "String", "string", J::String, true),
            field("manual", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            field(
                "effort",
                W::U16,
                &[],
                "ProductModelEffort",
                "\"default\" | \"minimal\" | \"low\" | \"medium\" | \"high\" | \"xhigh\" | \"max\" | \"ultra\"",
                J::Enum(&["default", "minimal", "low", "medium", "high", "xhigh", "max", "ultra"]),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchImageRequest",
        rust_type: "WorkbenchImageRequest",
        fields: &[
            nested("query", "WorkbenchQuery"),
            revision("revision"),
            id("artifact", "ArtifactId"),
            id("provider", "ProviderProfileId"),
            nested("model", "WorkbenchImageModel"),
            field(
                "label",
                W::Utf8,
                &[B::WorkbenchImageLabelBytes],
                "WorkbenchImageLabel",
                "string",
                J::String,
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchImageMetadata",
        rust_type: "WorkbenchImageMetadata",
        fields: &[
            field("digest", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
            field(
                "bytes",
                W::U64,
                &[B::NonZero, B::WorkbenchImageBytes],
                "u64",
                "UInt64",
                J::U64String,
                true,
            ),
            field(
                "format",
                W::U16,
                &[],
                "WorkbenchImageFormat",
                "\"png\" | \"jpeg\" | \"gif\" | \"webp\"",
                J::Enum(&["png", "jpeg", "gif", "webp"]),
                true,
            ),
            field(
                "width",
                W::U32,
                &[B::NonZero, B::WorkbenchImageSide],
                "u32",
                "number",
                J::U32,
                true,
            ),
            field(
                "height",
                W::U32,
                &[B::NonZero, B::WorkbenchImageSide],
                "u32",
                "number",
                J::U32,
                true,
            ),
            field(
                "frames",
                W::U32,
                &[B::NonZero, B::WorkbenchImageFrames],
                "u32",
                "number",
                J::U32,
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchImagePreview",
        rust_type: "WorkbenchImagePreview",
        fields: &[
            nested("request", "WorkbenchImageRequest"),
            nested("image", "WorkbenchImageMetadata"),
            revision("providerRevision"),
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
        name: "WorkbenchAttachImageIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            field(
                "kind",
                W::U16,
                &[],
                "WorkbenchIntent",
                "\"attachImage\"",
                J::Enum(&["attachImage"]),
                true,
            ),
            nested("preview", "WorkbenchImagePreview"),
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
        name: "WorkbenchSelectImageIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            field(
                "kind",
                W::U16,
                &[],
                "WorkbenchIntent",
                "\"selectImage\"",
                J::Enum(&["selectImage"]),
                true,
            ),
            id("attachment", "ControlOperationId"),
            field("selected", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
        ],
    },
];

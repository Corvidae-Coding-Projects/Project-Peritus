//! Exact retained-image metadata page descriptors.

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
pub(super) const IMAGE_PAGE_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "WorkbenchImageQuery",
        rust_type: "WorkbenchImageQuery",
        fields: &[
            nested("query", "WorkbenchQuery"),
            field("revision", W::U64, &[], "u64", "UInt64", J::U64String, true),
            field("offset", W::U32, &[], "u32", "number", J::U32, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchImageRow",
        rust_type: "WorkbenchImageRow",
        fields: &[
            id("operation", "ControlOperationId"),
            id("artifact", "ArtifactId"),
            field(
                "label",
                W::Utf8,
                &[B::WorkbenchImageLabelBytes],
                "WorkbenchImageLabel",
                "string",
                J::String,
                true,
            ),
            nested("image", "WorkbenchImageMetadata"),
            nested("source", "WorkbenchInputRow"),
            field("selected", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            field("eligible", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchImagePage",
        rust_type: "WorkbenchImagePage",
        fields: &[
            nested("query", "WorkbenchImageQuery"),
            field("total", W::U32, &[], "u32", "number", J::U32, true),
            field(
                "rows",
                W::Sequence,
                &[B::WorkbenchImagePage],
                "Vec<WorkbenchImageRow>",
                "readonly WorkbenchImageRow[]",
                J::ArrayRef("WorkbenchImageRow"),
                true,
            ),
        ],
    },
];

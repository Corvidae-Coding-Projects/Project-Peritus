//! Field shapes for checkpoint schema descriptors.

use super::super::super::{
    AppFieldDescriptor, CanonicalWireType as W, FieldBound as B, JsonShape as J, field,
};

pub(super) const VERSION_TYPES: &[&str] =
    &["WorkbenchCheckpointAbsentVersion", "WorkbenchCheckpointPresentVersion"];

pub(super) const fn nested(name: &'static str, ty: &'static str) -> AppFieldDescriptor {
    field(name, W::Struct, &[], ty, ty, J::Ref(ty), true)
}
pub(super) const fn nested_optional(name: &'static str, ty: &'static str) -> AppFieldDescriptor {
    field(name, W::Option, &[], ty, ty, J::Ref(ty), false)
}
pub(super) const fn id(name: &'static str) -> AppFieldDescriptor {
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
pub(super) const fn revision(name: &'static str, required: bool) -> AppFieldDescriptor {
    field(name, W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, required)
}
pub(super) const fn version(name: &'static str, required: bool) -> AppFieldDescriptor {
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
pub(super) const fn path(name: &'static str) -> AppFieldDescriptor {
    field(name, W::Utf8, &[B::WorkbenchFilePathBytes], "String", "string", J::String, true)
}
pub(super) const fn strings(name: &'static str, restore: bool) -> AppFieldDescriptor {
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
pub(super) const fn page_strings(name: &'static str) -> AppFieldDescriptor {
    field(
        name,
        W::Sequence,
        &[B::CodecCollectionItems, B::WorkbenchCheckpointTextBytes],
        "Vec<String>",
        "readonly string[]",
        J::StringArray,
        true,
    )
}
pub(super) const fn u64_field(name: &'static str) -> AppFieldDescriptor {
    field(name, W::U64, &[], "u64", "UInt64", J::U64String, true)
}
pub(super) const fn kind(
    value: &'static str,
    values: &'static [&'static str],
) -> AppFieldDescriptor {
    field("kind", W::U16, &[], "WorkbenchIntent", value, J::Enum(values), true)
}

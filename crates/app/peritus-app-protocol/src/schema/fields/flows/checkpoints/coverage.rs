//! Additive typed-node and selected-range schema descriptors.

use super::{AppFieldDescriptor, AppTypeDescriptor, B, J, W, field};

pub(super) const DIRECTORY: AppTypeDescriptor = AppTypeDescriptor {
    name: "WorkbenchCheckpointDirectoryVersion",
    rust_type: "WorkbenchCheckpointVersion",
    fields: &[
        field(
            "kind",
            W::U16,
            &[],
            "WorkbenchCheckpointVersion",
            "\"emptyDirectory\"",
            J::Enum(&["emptyDirectory"]),
            true,
        ),
        field("permissions", W::U16, &[], "u16", "number", J::U16, true),
    ],
};

pub(super) const RANGE: AppTypeDescriptor = AppTypeDescriptor {
    name: "WorkbenchCheckpointRange",
    rust_type: "WorkbenchCheckpointRange",
    fields: &[
        field(
            "selection",
            W::Struct,
            &[],
            "WorkbenchFileRange",
            "WorkbenchFileRangeBytes | WorkbenchFileRangeLines",
            J::OneOfRef(&["WorkbenchFileRangeBytes", "WorkbenchFileRangeLines"]),
            true,
        ),
        field("start", W::U64, &[], "u64", "UInt64", J::U64String, true),
        field("end", W::U64, &[], "u64", "UInt64", J::U64String, true),
    ],
};

pub(super) const fn ranges() -> AppFieldDescriptor {
    field(
        "ranges",
        W::Sequence,
        &[B::SortedUnique],
        "Vec<WorkbenchCheckpointRange>",
        "readonly WorkbenchCheckpointRange[]",
        J::ArrayRef("WorkbenchCheckpointRange"),
        false,
    )
}

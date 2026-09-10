//! Queue DTO metadata in exact canonical field order.

use super::super::{
    AppFieldDescriptor, AppTypeDescriptor, CanonicalWireType as W, FieldBound as B, JsonShape as J,
    field,
};

const fn input_id(name: &'static str) -> AppFieldDescriptor {
    field(
        name,
        W::Identifier,
        &[B::NonZero],
        "WorkbenchInputId",
        "WorkbenchInputId",
        J::Identifier,
        true,
    )
}
const fn selected(name: &'static str) -> AppFieldDescriptor {
    field(
        name,
        W::Struct,
        &[],
        "WorkbenchInputSelection",
        "WorkbenchInputSelection",
        J::Ref("WorkbenchInputSelection"),
        true,
    )
}
const TEXT: AppFieldDescriptor = field(
    "text",
    W::Utf8,
    &[B::WorkbenchInputBytes],
    "WorkbenchInputText",
    "string",
    J::String,
    true,
);
const DEPENDENCIES: AppFieldDescriptor = field(
    "dependencies",
    W::Sequence,
    &[B::WorkbenchInputDependencies],
    "WorkbenchInputOrder",
    "readonly WorkbenchInputId[]",
    J::IdentifierArray,
    true,
);
const fn kind(name: &'static str, values: &'static [&'static str]) -> AppFieldDescriptor {
    field("kind", W::U16, &[], "WorkbenchQueueIntent", name, J::Enum(values), true)
}

pub(super) const INPUT_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "WorkbenchInputSelection",
        rust_type: "WorkbenchInputSelection",
        fields: &[
            input_id("id"),
            field("revision", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchNewInput",
        rust_type: "WorkbenchNewInput",
        fields: &[input_id("id"), TEXT, DEPENDENCIES],
    },
    AppTypeDescriptor {
        name: "WorkbenchEnqueueIntent",
        rust_type: "WorkbenchQueueIntent",
        fields: &[
            kind("\"enqueue\"", &["enqueue"]),
            field(
                "input",
                W::Struct,
                &[],
                "WorkbenchNewInput",
                "WorkbenchNewInput",
                J::Ref("WorkbenchNewInput"),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchEditInputIntent",
        rust_type: "WorkbenchQueueIntent",
        fields: &[kind("\"edit\"", &["edit"]), selected("selected"), TEXT],
    },
    AppTypeDescriptor {
        name: "WorkbenchCorrectInputIntent",
        rust_type: "WorkbenchQueueIntent",
        fields: &[kind("\"correct\"", &["correct"]), selected("original"), input_id("id"), TEXT],
    },
    AppTypeDescriptor {
        name: "WorkbenchHoldInputIntent",
        rust_type: "WorkbenchQueueIntent",
        fields: &[
            kind("\"hold\"", &["hold"]),
            selected("selected"),
            field("held", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchWithdrawInputIntent",
        rust_type: "WorkbenchQueueIntent",
        fields: &[kind("\"withdraw\"", &["withdraw"]), selected("selected")],
    },
    AppTypeDescriptor {
        name: "WorkbenchReorderInputIntent",
        rust_type: "WorkbenchQueueIntent",
        fields: &[
            kind("\"reorder\"", &["reorder"]),
            field(
                "order",
                W::Sequence,
                &[B::WorkbenchInputs],
                "WorkbenchInputOrder",
                "readonly WorkbenchInputId[]",
                J::IdentifierArray,
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchQueueControlIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            field("kind", W::U16, &[], "WorkbenchIntent", "\"queue\"", J::Enum(&["queue"]), true),
            field(
                "queue",
                W::Struct,
                &[],
                "WorkbenchQueueIntent",
                "WorkbenchEnqueueIntent | WorkbenchEditInputIntent | WorkbenchCorrectInputIntent | WorkbenchHoldInputIntent | WorkbenchWithdrawInputIntent | WorkbenchReorderInputIntent",
                J::OneOfRef(&[
                    "WorkbenchEnqueueIntent",
                    "WorkbenchEditInputIntent",
                    "WorkbenchCorrectInputIntent",
                    "WorkbenchHoldInputIntent",
                    "WorkbenchWithdrawInputIntent",
                    "WorkbenchReorderInputIntent",
                ]),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchInputRow",
        rust_type: "WorkbenchInputRow",
        fields: &[
            selected("selected"),
            TEXT,
            field(
                "state",
                W::U16,
                &[],
                "WorkbenchInputState",
                "\"queued\" | \"held\" | \"incorporated\" | \"superseded\" | \"withdrawn\"",
                J::Enum(&["queued", "held", "incorporated", "superseded", "withdrawn"]),
                true,
            ),
            DEPENDENCIES,
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchQueueQuery",
        rust_type: "WorkbenchQueueQuery",
        fields: &[
            field(
                "query",
                W::Struct,
                &[],
                "WorkbenchQuery",
                "WorkbenchQuery",
                J::Ref("WorkbenchQuery"),
                true,
            ),
            field("revision", W::U64, &[], "u64", "UInt64", J::U64String, true),
            field("offset", W::U32, &[B::WorkbenchInputs], "u32", "number", J::U32, true),
            field("history", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchQueuePage",
        rust_type: "WorkbenchQueuePage",
        fields: &[
            field(
                "query",
                W::Struct,
                &[],
                "WorkbenchQueueQuery",
                "WorkbenchQueueQuery",
                J::Ref("WorkbenchQueueQuery"),
                true,
            ),
            field("total", W::U32, &[B::WorkbenchInputs], "u32", "number", J::U32, true),
            field(
                "rows",
                W::Sequence,
                &[B::WorkbenchInputPage],
                "Vec<WorkbenchInputRow>",
                "readonly WorkbenchInputRow[]",
                J::ArrayRef("WorkbenchInputRow"),
                true,
            ),
        ],
    },
];

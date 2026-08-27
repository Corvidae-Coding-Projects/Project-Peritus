//! Terminal attachment flow descriptors.

use super::super::super::{
    AppTypeDescriptor, CanonicalWireType, FieldBound as B, JsonShape as J, field,
};

pub(super) const TERMINAL_BINDING: AppTypeDescriptor = AppTypeDescriptor {
    name: "TerminalBinding",
    rust_type: "TerminalBinding",
    fields: &[
        field(
            "attachmentId",
            CanonicalWireType::Identifier,
            &[B::NonZero],
            "TerminalAttachmentId",
            "TerminalAttachmentId",
            J::Identifier,
            true,
        ),
        field(
            "processId",
            CanonicalWireType::Identifier,
            &[B::NonZero],
            "ProcessId",
            "ProcessId",
            J::Identifier,
            true,
        ),
        field(
            "originatingRequestId",
            CanonicalWireType::Identifier,
            &[B::NonZero, B::EnvelopeBinding],
            "RequestId",
            "RequestId",
            J::Identifier,
            true,
        ),
    ],
};

pub(super) const TERMINAL_INPUT: AppTypeDescriptor = AppTypeDescriptor {
    name: "TerminalInput",
    rust_type: "TerminalInput",
    fields: &[
        field(
            "binding",
            CanonicalWireType::Struct,
            &[],
            "TerminalBinding",
            "TerminalBinding",
            J::Ref("TerminalBinding"),
            true,
        ),
        field(
            "bytes",
            CanonicalWireType::Bytes,
            &[B::TerminalChunkBytes],
            "Vec<u8>",
            "Base64Bytes",
            J::Base64,
            true,
        ),
    ],
};

pub(super) const TERMINAL_RESIZE: AppTypeDescriptor = AppTypeDescriptor {
    name: "TerminalResize",
    rust_type: "TerminalResize",
    fields: &[
        field(
            "binding",
            CanonicalWireType::Struct,
            &[],
            "TerminalBinding",
            "TerminalBinding",
            J::Ref("TerminalBinding"),
            true,
        ),
        field("columns", CanonicalWireType::U16, &[B::NonZero], "u16", "number", J::U16, true),
        field("rows", CanonicalWireType::U16, &[B::NonZero], "u16", "number", J::U16, true),
    ],
};

pub(super) const TERMINAL_DETACH: AppTypeDescriptor = AppTypeDescriptor {
    name: "TerminalDetach",
    rust_type: "TerminalDetach",
    fields: &[
        field(
            "binding",
            CanonicalWireType::Struct,
            &[],
            "TerminalBinding",
            "TerminalBinding",
            J::Ref("TerminalBinding"),
            true,
        ),
        field(
            "correlationId",
            CanonicalWireType::Identifier,
            &[B::NonZero],
            "CorrelationId",
            "CorrelationId",
            J::Identifier,
            true,
        ),
    ],
};

pub(super) const TERMINAL_CANCELLATION: AppTypeDescriptor = AppTypeDescriptor {
    name: "TerminalCancellation",
    rust_type: "TerminalCancellation",
    fields: &[
        field(
            "binding",
            CanonicalWireType::Struct,
            &[],
            "TerminalBinding",
            "TerminalBinding",
            J::Ref("TerminalBinding"),
            true,
        ),
        field(
            "correlationId",
            CanonicalWireType::Identifier,
            &[B::NonZero, B::EnvelopeBinding],
            "CorrelationId",
            "CorrelationId",
            J::Identifier,
            true,
        ),
    ],
};

pub(super) const TERMINAL_OUTPUT: AppTypeDescriptor = AppTypeDescriptor {
    name: "TerminalOutput",
    rust_type: "TerminalOutput",
    fields: &[
        field(
            "binding",
            CanonicalWireType::Struct,
            &[],
            "TerminalBinding",
            "TerminalBinding",
            J::Ref("TerminalBinding"),
            true,
        ),
        field(
            "sequence",
            CanonicalWireType::U64,
            &[B::Contiguous],
            "u64",
            "UInt64",
            J::U64String,
            true,
        ),
        field(
            "offset",
            CanonicalWireType::U64,
            &[B::Contiguous],
            "u64",
            "UInt64",
            J::U64String,
            true,
        ),
        field(
            "stream",
            CanonicalWireType::U8,
            &[],
            "TerminalStream",
            "\"stdout\" | \"stderr\" | \"terminal\"",
            J::Enum(&["stdout", "stderr", "terminal"]),
            true,
        ),
        field(
            "bytes",
            CanonicalWireType::Bytes,
            &[B::TerminalChunkBytes],
            "Vec<u8>",
            "Base64Bytes",
            J::Base64,
            true,
        ),
    ],
};

pub(super) const TERMINAL_EXIT: AppTypeDescriptor = AppTypeDescriptor {
    name: "TerminalExit",
    rust_type: "TerminalExit",
    fields: &[
        field(
            "binding",
            CanonicalWireType::Struct,
            &[],
            "TerminalBinding",
            "TerminalBinding",
            J::Ref("TerminalBinding"),
            true,
        ),
        field(
            "nextSequence",
            CanonicalWireType::U64,
            &[B::Contiguous],
            "u64",
            "UInt64",
            J::U64String,
            true,
        ),
        field(
            "finalOffset",
            CanonicalWireType::U64,
            &[B::Contiguous],
            "u64",
            "UInt64",
            J::U64String,
            true,
        ),
        field(
            "disposition",
            CanonicalWireType::U8,
            &[],
            "TerminalExitDisposition",
            "TerminalExitDisposition",
            J::Ref("TerminalExitDisposition"),
            true,
        ),
    ],
};

pub(super) const TERMINAL_EXIT_DISPOSITION: AppTypeDescriptor = AppTypeDescriptor {
    name: "TerminalExitDisposition",
    rust_type: "TerminalExitDisposition",
    fields: &[
        field(
            "kind",
            CanonicalWireType::U8,
            &[],
            "TerminalExitDisposition",
            "\"code\" | \"signal\" | \"unknown\"",
            J::Enum(&["code", "signal", "unknown"]),
            true,
        ),
        field("value", CanonicalWireType::I32, &[], "Option<i32>", "number", J::I32, false),
    ],
};

//! Exact approval-first project-initialization fields.

use super::super::{
    AppFieldDescriptor, AppTypeDescriptor, CanonicalWireType as W, FieldBound as B, JsonShape as J,
    field,
};

const QUERY: AppFieldDescriptor = field(
    "query",
    W::Struct,
    &[],
    "WorkbenchQuery",
    "WorkbenchQuery",
    J::Ref("WorkbenchQuery"),
    true,
);
const REVISION: AppFieldDescriptor =
    field("revision", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true);
const PATH: AppFieldDescriptor =
    field("path", W::Utf8, &[B::WorkbenchFilePathBytes], "String", "string", J::String, true);

/// Initialization discovery, proposal, and explicit-apply types.
pub(super) const INIT_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "InitDiscoveryRequest",
        rust_type: "InitDiscoveryRequest",
        fields: &[QUERY, REVISION],
    },
    AppTypeDescriptor {
        name: "InitSourceObservation",
        rust_type: "InitSourceObservation",
        fields: &[
            PATH,
            field(
                "kind",
                W::U16,
                &[],
                "InitSourceKind",
                "\"manifest\" | \"documentation\" | \"instructions\" | \"commandConfig\"",
                J::Enum(&["manifest", "documentation", "instructions", "commandConfig"]),
                true,
            ),
            field("digest", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
            field("bytes", W::U64, &[B::CodecStringBytes], "u64", "UInt64", J::U64String, true),
        ],
    },
    AppTypeDescriptor {
        name: "InitInstructionPatch",
        rust_type: "InitInstructionPatch",
        fields: &[
            PATH,
            field("hasOriginal", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            field(
                "originalContent",
                W::Utf8,
                &[B::CodecStringBytes],
                "Option<String>",
                "string",
                J::String,
                false,
            ),
            field("hasPreconditionDigest", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            field(
                "preconditionDigest",
                W::Digest,
                &[],
                "Option<Sha256Digest>",
                "Sha256Digest",
                J::Digest,
                false,
            ),
            field(
                "preconditionBytes",
                W::U64,
                &[B::CodecStringBytes],
                "u64",
                "UInt64",
                J::U64String,
                true,
            ),
            field(
                "mode",
                W::U16,
                &[],
                "InitFileMode",
                "\"regular\" | \"executable\"",
                J::Enum(&["regular", "executable"]),
                true,
            ),
            field(
                "proposedContent",
                W::Utf8,
                &[B::CodecStringBytes],
                "String",
                "string",
                J::String,
                true,
            ),
            field("diff", W::Utf8, &[B::CodecStringBytes], "String", "string", J::String, true),
        ],
    },
    AppTypeDescriptor {
        name: "InitCommand",
        rust_type: "InitCommand",
        fields: &[
            field(
                "kind",
                W::U16,
                &[],
                "InitCommandKind",
                "\"build\" | \"test\" | \"lint\" | \"launch\"",
                J::Enum(&["build", "test", "lint", "launch"]),
                true,
            ),
            field(
                "source",
                W::Utf8,
                &[B::WorkbenchFilePathBytes],
                "String",
                "string",
                J::String,
                true,
            ),
            field(
                "executable",
                W::Utf8,
                &[B::CodecStringBytes],
                "String",
                "string",
                J::String,
                true,
            ),
            field(
                "arguments",
                W::Sequence,
                &[B::CodecCollectionItems, B::CodecStringBytes],
                "Vec<String>",
                "readonly string[]",
                J::StringArray,
                true,
            ),
            field(
                "verification",
                W::U16,
                &[],
                "InitCommandVerification",
                "\"unverified\"",
                J::Enum(&["unverified"]),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "InitProposal",
        rust_type: "InitProposal",
        fields: &[
            QUERY,
            REVISION,
            field("folderDigest", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
            field(
                "sources",
                W::Sequence,
                &[B::CodecCollectionItems, B::SortedUnique],
                "Vec<InitSourceObservation>",
                "readonly InitSourceObservation[]",
                J::ArrayRef("InitSourceObservation"),
                true,
            ),
            field(
                "patch",
                W::Struct,
                &[],
                "InitInstructionPatch",
                "InitInstructionPatch",
                J::Ref("InitInstructionPatch"),
                true,
            ),
            field(
                "commands",
                W::Sequence,
                &[B::CodecCollectionItems, B::SortedUnique],
                "Vec<InitCommand>",
                "readonly InitCommand[]",
                J::ArrayRef("InitCommand"),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchInitApplyIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            field(
                "kind",
                W::U16,
                &[],
                "WorkbenchIntent",
                "\"applyInitDiff\"",
                J::Enum(&["applyInitDiff"]),
                true,
            ),
            field(
                "proposal",
                W::Struct,
                &[],
                "InitProposal",
                "InitProposal",
                J::Ref("InitProposal"),
                true,
            ),
        ],
    },
];

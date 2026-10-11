//! Additive immutable initialization artifact contracts.
use super::super::{
    AppFieldDescriptor, AppTypeDescriptor, CanonicalWireType as W, FieldBound as B, JsonShape as J,
    field,
};
const fn nested(name: &'static str, ty: &'static str, required: bool) -> AppFieldDescriptor {
    field(name, W::Struct, &[], ty, ty, J::Ref(ty), required)
}
const fn flag(name: &'static str) -> AppFieldDescriptor {
    field(name, W::Boolean, &[], "bool", "boolean", J::Boolean, true)
}
const fn number(name: &'static str, required: bool) -> AppFieldDescriptor {
    field(name, W::U64, &[], "u64", "UInt64", J::U64String, required)
}
pub(super) const INIT_ARTIFACT_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "InitContentReference",
        rust_type: "InitContentReference",
        fields: &[
            field("digest", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
            number("bytes", true),
        ],
    },
    AppTypeDescriptor {
        name: "InitSourceSelection",
        rust_type: "InitSourceSelection",
        fields: &[
            field(
                "path",
                W::Utf8,
                &[B::WorkbenchFilePathBytes],
                "String",
                "string",
                J::String,
                true,
            ),
            field(
                "kind",
                W::U16,
                &[],
                "InitSourceKind",
                "\"manifest\" | \"documentation\" | \"instructions\" | \"commandConfig\"",
                J::Enum(&["manifest", "documentation", "instructions", "commandConfig"]),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "InitArtifactDiscovery",
        rust_type: "InitArtifactDiscovery",
        fields: &[
            nested("request", "InitDiscoveryRequest", true),
            flag("hasPrevious"),
            nested("previous", "InitContentReference", false),
            flag("hasSource"),
            nested("source", "InitSourceSelection", false),
            flag("hasCommand"),
            number("command", false),
        ],
    },
    AppTypeDescriptor {
        name: "InitArtifactProposal",
        rust_type: "InitArtifactProposal",
        fields: &[
            nested("request", "InitDiscoveryRequest", true),
            nested("manifest", "InitContentReference", true),
            nested("review", "InitContentReference", true),
        ],
    },
    AppTypeDescriptor {
        name: "InitArtifactPageRequest",
        rust_type: "InitArtifactPageRequest",
        fields: &[
            nested("proposal", "InitArtifactProposal", true),
            number("offset", true),
            field(
                "maximum",
                W::U32,
                &[B::NonZero, B::ArtifactChunkBytes],
                "u32",
                "number",
                J::U32,
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "InitArtifactPage",
        rust_type: "InitArtifactPage",
        fields: &[
            nested("request", "InitArtifactPageRequest", true),
            field(
                "bytes",
                W::Bytes,
                &[B::ArtifactChunkBytes],
                "Vec<u8>",
                "string",
                J::Base64,
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchInitArtifactIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            field(
                "kind",
                W::U16,
                &[],
                "WorkbenchIntent",
                "\"applyInitArtifact\"",
                J::Enum(&["applyInitArtifact"]),
                true,
            ),
            nested("proposal", "InitArtifactProposal", true),
        ],
    },
];

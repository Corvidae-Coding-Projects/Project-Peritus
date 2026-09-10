//! Bounded doctor wire and projection metadata.

use super::super::{
    AppTypeDescriptor, CanonicalWireType as W, FieldBound as B, JsonShape as J, field,
};

pub(super) const DOCTOR_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "DoctorQuery",
        rust_type: "DoctorQuery",
        fields: &[
            field(
                "workspace",
                W::Identifier,
                &[B::NonZero],
                "WorkspaceId",
                "WorkspaceId",
                J::Identifier,
                true,
            ),
            field(
                "provider",
                W::Option,
                &[B::NonZero],
                "Option<ProviderProfileId>",
                "ProviderProfileId",
                J::Identifier,
                false,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "DoctorFinding",
        rust_type: "DoctorFinding",
        fields: &[
            field("check", W::Utf8, &[B::DoctorCheckBytes], "String", "string", J::String, true),
            field(
                "status",
                W::U16,
                &[],
                "DoctorStatus",
                "\"healthy\" | \"warning\" | \"blocked\" | \"unsupported\"",
                J::Enum(&["healthy", "warning", "blocked", "unsupported"]),
                true,
            ),
            field(
                "observation",
                W::Utf8,
                &[B::DoctorTextBytes],
                "String",
                "string",
                J::String,
                true,
            ),
            field("action", W::Utf8, &[B::DoctorTextBytes], "String", "string", J::String, true),
        ],
    },
    AppTypeDescriptor {
        name: "DoctorReport",
        rust_type: "DoctorReport",
        fields: &[
            field(
                "query",
                W::Struct,
                &[],
                "DoctorQuery",
                "DoctorQuery",
                J::Ref("DoctorQuery"),
                true,
            ),
            field(
                "findings",
                W::Sequence,
                &[B::DoctorFindings],
                "Vec<DoctorFinding>",
                "readonly DoctorFinding[]",
                J::ArrayRef("DoctorFinding"),
                true,
            ),
        ],
    },
];

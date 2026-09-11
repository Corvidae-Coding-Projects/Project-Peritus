//! Effective workspace permission and restriction metadata.

use super::super::{
    AppTypeDescriptor, CanonicalWireType as W, FieldBound as B, JsonShape as J, field,
};

pub(super) const PERMISSION_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "WorkbenchPermissionChange",
        rust_type: "WorkbenchPermissionChange",
        fields: &[
            field("expectedAuthorityRevision", W::U64, &[], "u64", "UInt64", J::U64String, true),
            field(
                "capability",
                W::U16,
                &[],
                "WorkbenchPermissionCapability",
                "\"read\" | \"write\" | \"process\" | \"network\"",
                J::Enum(&["read", "write", "process", "network"]),
                true,
            ),
            field("allowed", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchPermissionIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            field(
                "kind",
                W::U16,
                &[],
                "WorkbenchIntent",
                "\"setPermissions\"",
                J::Enum(&["setPermissions"]),
                true,
            ),
            field(
                "change",
                W::Struct,
                &[],
                "WorkbenchPermissionChange",
                "WorkbenchPermissionChange",
                J::Ref("WorkbenchPermissionChange"),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchPermissionEntry",
        rust_type: "WorkbenchPermissionEntry",
        fields: &[
            field(
                "capability",
                W::U16,
                &[],
                "WorkbenchPermissionCapability",
                "\"read\" | \"write\" | \"process\" | \"network\"",
                J::Enum(&["read", "write", "process", "network"]),
                true,
            ),
            field("hostAllowed", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            field("effectiveAllowed", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            field(
                "provenance",
                W::U16,
                &[],
                "WorkbenchPermissionProvenance",
                "\"workspaceHostPolicy\" | \"toolHostPolicy\" | \"providerHostPolicy\" | \"userRestriction\"",
                J::Enum(&[
                    "workspaceHostPolicy",
                    "toolHostPolicy",
                    "providerHostPolicy",
                    "userRestriction",
                ]),
                true,
            ),
            field(
                "explicitApprovalStillRequired",
                W::Boolean,
                &[],
                "bool",
                "boolean",
                J::Boolean,
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchPermissions",
        rust_type: "WorkbenchPermissions",
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
            field(
                "conversationRevision",
                W::U64,
                &[B::NonZero],
                "u64",
                "UInt64",
                J::U64String,
                true,
            ),
            field("authorityRevision", W::U64, &[], "u64", "UInt64", J::U64String, true),
            field(
                "trust",
                W::U16,
                &[],
                "WorkbenchWorkspaceTrust",
                "\"managed\" | \"directReadOnly\" | \"directWritable\"",
                J::Enum(&["managed", "directReadOnly", "directWritable"]),
                true,
            ),
            field(
                "entries",
                W::Sequence,
                &[B::CodecCollectionItems],
                "[WorkbenchPermissionEntry; 4]",
                "WorkbenchPermissionEntry[]",
                J::ArrayRef("WorkbenchPermissionEntry"),
                true,
            ),
        ],
    },
];

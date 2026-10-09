//! Preview launch, selected-window capture, and independent result-evidence descriptors.

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
const fn optional_id(
    name: &'static str,
    rust_type: &'static str,
    typescript_type: &'static str,
) -> AppFieldDescriptor {
    field(name, W::Option, &[], rust_type, typescript_type, J::Identifier, false)
}
const fn launch_text(name: &'static str) -> AppFieldDescriptor {
    field(
        name,
        W::Utf8,
        &[B::WorkbenchLaunchTextBytes],
        "WorkbenchLaunchText",
        "string",
        J::String,
        true,
    )
}
const fn intent_kind(
    typescript_type: &'static str,
    values: &'static [&'static str],
) -> AppFieldDescriptor {
    field("kind", W::U16, &[], "WorkbenchIntent", typescript_type, J::Enum(values), true)
}
const fn feedback_kind() -> AppFieldDescriptor {
    field(
        "feedback",
        W::U16,
        &[],
        "WorkbenchReviewFeedback",
        "\"explain\" | \"requestRevision\" | \"keepBehavior\" | \"leaveAlone\"",
        J::Enum(&["explain", "requestRevision", "keepBehavior", "leaveAlone"]),
        true,
    )
}

mod results;
pub(super) use results::RESULT_TYPES;

pub(super) const LAUNCH_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "WorkbenchResultQuery",
        rust_type: "WorkbenchResultQuery",
        fields: &[nested("query", "WorkbenchQuery"), id("run", "RunId")],
    },
    AppTypeDescriptor {
        name: "WorkbenchLaunchSource",
        rust_type: "WorkbenchLaunchSource",
        fields: &[
            field(
                "kind",
                W::U16,
                &[],
                "WorkbenchLaunchSourceKind",
                "\"managedCandidate\" | \"plainFolderFile\"",
                J::Enum(&["managedCandidate", "plainFolderFile"]),
                true,
            ),
            launch_text("path"),
            field("digest", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchBuildIdentity",
        rust_type: "WorkbenchBuildIdentity",
        fields: &[
            launch_text("path"),
            field("digest", W::Digest, &[], "Sha256Digest", "Sha256Digest", J::Digest, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchLaunchProfile",
        rust_type: "WorkbenchLaunchProfile",
        fields: &[
            id("run", "RunId"),
            launch_text("executable"),
            field(
                "arguments",
                W::Sequence,
                &[B::WorkbenchLaunchArguments, B::WorkbenchLaunchTextBytes],
                "Vec<WorkbenchLaunchText>",
                "readonly string[]",
                J::StringArray,
                true,
            ),
            launch_text("workingDirectory"),
            field(
                "environment",
                W::Sequence,
                &[B::WorkbenchLaunchEnvironment, B::WorkbenchLaunchTextBytes, B::SortedUnique],
                "Vec<WorkbenchLaunchText>",
                "readonly string[]",
                J::StringArray,
                true,
            ),
            nested("source", "WorkbenchLaunchSource"),
            field(
                "build",
                W::Option,
                &[],
                "Option<WorkbenchBuildIdentity>",
                "WorkbenchBuildIdentity",
                J::Ref("WorkbenchBuildIdentity"),
                false,
            ),
            field(
                "readinessMillis",
                W::U64,
                &[B::NonZero, B::WorkbenchLaunchWallMillis],
                "u64",
                "UInt64",
                J::U64String,
                true,
            ),
            field(
                "wallMillis",
                W::U64,
                &[B::NonZero, B::WorkbenchLaunchWallMillis],
                "u64",
                "UInt64",
                J::U64String,
                true,
            ),
            field("interactive", W::Boolean, &[], "bool", "boolean", J::Boolean, true),
            field(
                "network",
                W::U16,
                &[],
                "WorkbenchPreviewNetwork",
                "\"inheritedHost\"",
                J::Enum(&["inheritedHost"]),
                true,
            ),
            field(
                "stopPolicy",
                W::U16,
                &[],
                "WorkbenchPreviewStopPolicy",
                "\"terminateOwnedTree\"",
                J::Enum(&["terminateOwnedTree"]),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchCaptureTarget",
        rust_type: "WorkbenchCaptureTarget",
        fields: &[
            field(
                "kind",
                W::U16,
                &[],
                "WorkbenchCaptureTarget",
                "\"x11Window\"",
                J::Enum(&["x11Window"]),
                true,
            ),
            field("window", W::U64, &[B::NonZero], "u64", "UInt64", J::U64String, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchCaptureRequest",
        rust_type: "WorkbenchCaptureRequest",
        fields: &[
            id("launch", "ControlOperationId"),
            nested("target", "WorkbenchCaptureTarget"),
            field(
                "consent",
                W::U16,
                &[],
                "WorkbenchCaptureConsent",
                "\"granted\" | \"denied\"",
                J::Enum(&["granted", "denied"]),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchArtifactRegion",
        rust_type: "WorkbenchArtifactRegion",
        fields: &[
            field("x", W::U32, &[], "u32", "number", J::U32, true),
            field("y", W::U32, &[], "u32", "number", J::U32, true),
            field("width", W::U32, &[B::NonZero], "u32", "number", J::U32, true),
            field("height", W::U32, &[B::NonZero], "u32", "number", J::U32, true),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchStartPreviewIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            intent_kind("\"startPreview\"", &["startPreview"]),
            nested("profile", "WorkbenchLaunchProfile"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchInteractPreviewIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            intent_kind("\"interactPreview\"", &["interactPreview"]),
            id("launch", "ControlOperationId"),
            field(
                "input",
                W::Bytes,
                &[B::WorkbenchPreviewInputBytes],
                "WorkbenchPreviewInput",
                "Base64Bytes",
                J::Base64,
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchCapturePreviewIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            intent_kind("\"capturePreview\"", &["capturePreview"]),
            nested("request", "WorkbenchCaptureRequest"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchStopPreviewIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            intent_kind("\"stopPreview\"", &["stopPreview"]),
            id("launch", "ControlOperationId"),
        ],
    },
    AppTypeDescriptor {
        name: "WorkbenchCheckPreviewIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            intent_kind("\"checkPreviewBehavior\"", &["checkPreviewBehavior"]),
            id("launch", "ControlOperationId"),
            launch_text("observed"),
            field(
                "note",
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
        name: "WorkbenchArtifactFeedbackIntent",
        rust_type: "WorkbenchIntent",
        fields: &[
            intent_kind("\"addArtifactFeedback\"", &["addArtifactFeedback"]),
            id("capture", "ControlOperationId"),
            feedback_kind(),
            field(
                "message",
                W::Utf8,
                &[B::WorkbenchInputBytes],
                "WorkbenchInputText",
                "string",
                J::String,
                true,
            ),
            field(
                "region",
                W::Option,
                &[],
                "Option<WorkbenchArtifactRegion>",
                "WorkbenchArtifactRegion",
                J::Ref("WorkbenchArtifactRegion"),
                false,
            ),
        ],
    },
];

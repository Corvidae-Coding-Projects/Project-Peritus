//! Prompt and approval flow descriptors.

use super::super::super::{
    AppTypeDescriptor, CanonicalWireType, FieldBound as B, JsonShape as J, field,
};

pub(super) const APPROVAL_CHALLENGE: AppTypeDescriptor = AppTypeDescriptor {
    name: "ApprovalChallenge",
    rust_type: "ApprovalChallenge",
    fields: &[
        field(
            "decisionCommandId",
            CanonicalWireType::Identifier,
            &[B::NonZero],
            "CommandId",
            "CommandId",
            J::Identifier,
            true,
        ),
        field(
            "registryRevision",
            CanonicalWireType::U64,
            &[B::NonZero],
            "RevisionNumber",
            "UInt64",
            J::U64String,
            true,
        ),
        field(
            "requestFrame",
            CanonicalWireType::Bytes,
            &[B::NonZero, B::CodecOpaqueBytes],
            "Vec<u8>",
            "Base64Bytes",
            J::Base64,
            true,
        ),
    ],
};

pub(super) const SIGNED_APPROVAL_DECISION_FRAME: AppTypeDescriptor = AppTypeDescriptor {
    name: "SignedApprovalDecisionFrame",
    rust_type: "SignedApprovalDecisionFrame",
    fields: &[field(
        "bytes",
        CanonicalWireType::Bytes,
        &[B::NonZero, B::CodecOpaqueBytes],
        "Vec<u8>",
        "Base64Bytes",
        J::Base64,
        true,
    )],
};

pub(super) const PROMPT_BINDING: AppTypeDescriptor = AppTypeDescriptor {
    name: "PromptBinding",
    rust_type: "PromptBinding",
    fields: &[
        field(
            "kind",
            CanonicalWireType::U8,
            &[],
            "PromptKind",
            "\"approval\" | \"user-input\"",
            J::Enum(&["approval", "user-input"]),
            true,
        ),
        field(
            "approvalChallenge",
            CanonicalWireType::Option,
            &[],
            "Option<ApprovalChallenge>",
            "ApprovalChallenge",
            J::Ref("ApprovalChallenge"),
            false,
        ),
        field(
            "correlation",
            CanonicalWireType::Struct,
            &[],
            "PromptCorrelation",
            "PromptCorrelation",
            J::Ref("PromptCorrelation"),
            true,
        ),
        field(
            "choices",
            CanonicalWireType::Sequence,
            &[B::PromptChoices],
            "Vec<PromptChoice>",
            "readonly PromptChoice[]",
            J::ArrayRef("PromptChoice"),
            true,
        ),
        field(
            "constraints",
            CanonicalWireType::Sequence,
            &[B::CodecCollectionItems],
            "Vec<PromptConstraint>",
            "readonly PromptConstraint[]",
            J::ArrayRef("PromptConstraint"),
            true,
        ),
    ],
};

pub(super) const PROMPT_ANSWER: AppTypeDescriptor = AppTypeDescriptor {
    name: "PromptAnswer",
    rust_type: "PromptAnswer",
    fields: &[
        field(
            "correlation",
            CanonicalWireType::Struct,
            &[],
            "PromptCorrelation",
            "PromptCorrelation",
            J::Ref("PromptCorrelation"),
            true,
        ),
        field(
            "answerKind",
            CanonicalWireType::U8,
            &[],
            "PromptAnswerPayload",
            "\"approval\" | \"user-input\"",
            J::Enum(&["approval", "user-input"]),
            true,
        ),
        field(
            "approvalAnswerKind",
            CanonicalWireType::U8,
            &[],
            "Option<ApprovalAnswer>",
            "\"signed-decision\" | \"cancel\"",
            J::Enum(&["signed-decision", "cancel"]),
            false,
        ),
        field(
            "signedDecisionFrame",
            CanonicalWireType::Option,
            &[B::CodecOpaqueBytes],
            "Option<SignedApprovalDecisionFrame>",
            "Base64Bytes",
            J::Base64,
            false,
        ),
        field(
            "rationale",
            CanonicalWireType::Option,
            &[B::CodecStringBytes],
            "Option<String>",
            "string",
            J::String,
            false,
        ),
        field(
            "inputKind",
            CanonicalWireType::U8,
            &[],
            "Option<UserInputValue>",
            "\"text\" | \"selection\" | \"confirmation\" | \"secret-reference\"",
            J::Enum(&["text", "selection", "confirmation", "secret-reference"]),
            false,
        ),
        field(
            "textValue",
            CanonicalWireType::Utf8,
            &[B::CodecStringBytes],
            "Option<String>",
            "string",
            J::String,
            false,
        ),
        field(
            "confirmationValue",
            CanonicalWireType::Boolean,
            &[],
            "Option<bool>",
            "boolean",
            J::Boolean,
            false,
        ),
    ],
};

pub(super) const PROMPT_CANCELLATION: AppTypeDescriptor = AppTypeDescriptor {
    name: "PromptCancellation",
    rust_type: "PromptCancellation",
    fields: &[
        field(
            "correlation",
            CanonicalWireType::Struct,
            &[],
            "PromptCorrelation",
            "PromptCorrelation",
            J::Ref("PromptCorrelation"),
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

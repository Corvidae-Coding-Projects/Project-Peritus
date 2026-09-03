//! Candidate qualification and terminal settlement metadata.

use super::{AppTypeDescriptor, B, J, W, field};

const STAGES: &[&str] =
    &["observed", "changed", "self-checked", "gates-passed", "review-pending", "qualified"];
const EVIDENCE_STATUSES: &[&str] = &["missing", "current", "failed", "stale"];
const EVIDENCE_RESULTS: &[&str] = &["satisfied", "unsatisfied"];
const DISPOSITIONS: &[&str] = &[
    "accepted",
    "candidate-available",
    "waiting-for-user",
    "failed-no-candidate",
    "cancelled",
    "recovery-required",
];
const CAUSES: &[&str] = &[
    "completed",
    "user-wait",
    "cancellation",
    "deadline",
    "provider",
    "context",
    "gate",
    "review",
    "repository",
    "adapter",
    "recovery",
    "internal-invariant",
];

pub(in crate::schema::fields) const SETTLEMENT_TYPES: &[AppTypeDescriptor] = &[
    AppTypeDescriptor {
        name: "CandidateIdentity",
        rust_type: "CandidateIdentity",
        fields: &[
            field("runId", W::Identifier, &[B::NonZero], "RunId", "RunId", J::Identifier, true),
            field(
                "workspaceId",
                W::Identifier,
                &[B::NonZero],
                "WorkspaceId",
                "WorkspaceId",
                J::Identifier,
                true,
            ),
            field(
                "candidateDigest",
                W::Digest,
                &[],
                "Sha256Digest",
                "Sha256Digest",
                J::Digest,
                true,
            ),
            field("conversationRevision", W::U64, &[], "u64", "UInt64", J::U64String, true),
            field(
                "checkpointSequence",
                W::U64,
                &[B::NonZero, B::Contiguous],
                "u64",
                "UInt64",
                J::U64String,
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "QualificationEvidenceRecord",
        rust_type: "EvidenceRecord<QualificationEvidence>",
        fields: &[
            field(
                "provenance",
                W::Struct,
                &[],
                "CandidateIdentity",
                "CandidateIdentity",
                J::Ref("CandidateIdentity"),
                true,
            ),
            field(
                "result",
                W::U16,
                &[],
                "QualificationEvidence",
                "QualificationEvidence",
                J::Enum(EVIDENCE_RESULTS),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "QualificationEvidenceStatus",
        rust_type: "EvidenceStatus<QualificationEvidence>",
        fields: &[
            field(
                "status",
                W::U16,
                &[],
                "EvidenceStatus",
                "EvidenceStatus",
                J::Enum(EVIDENCE_STATUSES),
                true,
            ),
            field(
                "record",
                W::Struct,
                &[],
                "EvidenceRecord<QualificationEvidence>",
                "QualificationEvidenceRecord",
                J::Ref("QualificationEvidenceRecord"),
                false,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "CandidateCheckpoint",
        rust_type: "CandidateCheckpoint",
        fields: &[
            field(
                "identity",
                W::Struct,
                &[],
                "CandidateIdentity",
                "CandidateIdentity",
                J::Ref("CandidateIdentity"),
                true,
            ),
            field("stage", W::U16, &[], "CandidateStage", "CandidateStage", J::Enum(STAGES), true),
            field(
                "gates",
                W::Struct,
                &[],
                "EvidenceStatus<QualificationEvidence>",
                "QualificationEvidenceStatus",
                J::Ref("QualificationEvidenceStatus"),
                true,
            ),
            field(
                "obligations",
                W::Struct,
                &[],
                "EvidenceStatus<QualificationEvidence>",
                "QualificationEvidenceStatus",
                J::Ref("QualificationEvidenceStatus"),
                true,
            ),
            field(
                "review",
                W::Struct,
                &[],
                "EvidenceStatus<QualificationEvidence>",
                "QualificationEvidenceStatus",
                J::Ref("QualificationEvidenceStatus"),
                true,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "RunSettlement",
        rust_type: "RunSettlement",
        fields: &[
            field(
                "disposition",
                W::U16,
                &[],
                "RunDisposition",
                "RunDisposition",
                J::Enum(DISPOSITIONS),
                true,
            ),
            field(
                "cause",
                W::U16,
                &[],
                "SettlementCause",
                "SettlementCause",
                J::Enum(CAUSES),
                true,
            ),
            field(
                "checkpoint",
                W::Option,
                &[],
                "Option<CandidateCheckpoint>",
                "CandidateCheckpoint",
                J::Ref("CandidateCheckpoint"),
                false,
            ),
        ],
    },
    AppTypeDescriptor {
        name: "ProductRunSettlementSnapshot",
        rust_type: "ProductRunSettlementSnapshot",
        fields: &[
            field(
                "snapshot",
                W::Struct,
                &[],
                "ProductRunSnapshot",
                "ProductRunSnapshot",
                J::Ref("ProductRunSnapshot"),
                true,
            ),
            field(
                "settlement",
                W::Struct,
                &[],
                "RunSettlement",
                "RunSettlement",
                J::Ref("RunSettlement"),
                true,
            ),
        ],
    },
];

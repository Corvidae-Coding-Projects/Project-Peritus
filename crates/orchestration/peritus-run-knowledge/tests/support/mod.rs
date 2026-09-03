#![allow(dead_code, reason = "fixture helpers are shared across focused matrices")]

use peritus_role::HarnessRole;
use peritus_run_knowledge::{
    CurrentKnowledgeState, KnowledgeBinding, KnowledgeLimits, KnowledgeSection, KnowledgeSectionId,
    KnowledgeSectionKind, KnowledgeSourceId, RunKnowledgeSnapshot, SourceDigest,
};
use peritus_run_settlement::CandidateIdentity;
use peritus_types::{RunId, Sha256Digest, WorkspaceId};

#[derive(Clone, Copy)]
pub enum FixtureRevision {
    Baseline,
    SourceChanged,
    ConversationChanged,
    CandidateChanged,
}

pub fn limits() -> KnowledgeLimits {
    KnowledgeLimits::new(32, 64, 8, 8).expect("fixture limits")
}

pub fn section_id(byte: u8) -> KnowledgeSectionId {
    KnowledgeSectionId::new([byte; 16]).expect("section id")
}

pub fn source_id(byte: u8) -> KnowledgeSourceId {
    KnowledgeSourceId::new([byte; 16]).expect("source id")
}

pub const fn digest(byte: u8) -> Sha256Digest {
    Sha256Digest::new([byte; 32])
}

pub fn candidate(candidate_digest: u8, conversation: u64, sequence: u64) -> CandidateIdentity {
    CandidateIdentity::new(
        RunId::new([41; 16]).expect("run id"),
        WorkspaceId::new([42; 16]).expect("workspace id"),
        digest(candidate_digest),
        conversation,
        sequence,
    )
    .expect("candidate identity")
}

pub fn sources(source_a_digest: u8) -> Vec<SourceDigest> {
    vec![
        SourceDigest::new(source_id(1), digest(source_a_digest)),
        SourceDigest::new(source_id(2), digest(12)),
    ]
}

pub fn state(candidate: CandidateIdentity, source_a_digest: u8) -> CurrentKnowledgeState {
    CurrentKnowledgeState::new(candidate, sources(source_a_digest), limits())
        .expect("current state")
}

pub fn snapshot(
    candidate: CandidateIdentity,
    role: HarnessRole,
    source_a_digest: u8,
    revision: FixtureRevision,
) -> RunKnowledgeSnapshot {
    let source_a = SourceDigest::new(source_id(1), digest(source_a_digest));
    let source_b = SourceDigest::new(source_id(2), digest(12));
    let definitions = [
        (1, KnowledgeSectionKind::RepositoryInventory, vec![source_a], Vec::new()),
        (2, KnowledgeSectionKind::RelevantFileMap, vec![source_a], vec![section_id(1)]),
        (3, KnowledgeSectionKind::LiteralRequirementLedger, vec![source_b], Vec::new()),
        (4, KnowledgeSectionKind::DesignSection, vec![source_b], vec![section_id(3)]),
        (5, KnowledgeSectionKind::CompactedToolObservation, vec![source_a], vec![section_id(1)]),
        (6, KnowledgeSectionKind::ResolvedFinding, vec![source_a], vec![section_id(5)]),
        (7, KnowledgeSectionKind::CandidateEvidenceIndex, vec![source_a], vec![section_id(6)]),
        (
            8,
            KnowledgeSectionKind::NavigationSummary,
            vec![source_a, source_b],
            vec![section_id(4), section_id(7)],
        ),
    ];
    let sections = definitions
        .into_iter()
        .map(|(byte, kind, bound_sources, dependencies)| {
            let changed = match revision {
                FixtureRevision::Baseline => false,
                FixtureRevision::SourceChanged => {
                    bound_sources.iter().any(|source| source.source_id() == source_id(1))
                }
                FixtureRevision::ConversationChanged => {
                    kind.depends_on_conversation() || kind.depends_on_candidate()
                }
                FixtureRevision::CandidateChanged => kind.depends_on_candidate(),
            };
            let section_digest = digest(if changed { byte + 40 } else { byte });
            let binding = KnowledgeBinding::new(
                candidate,
                role,
                candidate.checkpoint_sequence(),
                bound_sources,
                limits(),
            )
            .expect("knowledge binding");
            KnowledgeSection::new(
                section_id(byte),
                kind,
                section_digest,
                binding,
                dependencies,
                limits(),
            )
            .expect("knowledge section")
        })
        .collect();
    RunKnowledgeSnapshot::new(
        candidate,
        role,
        section_id(1),
        section_id(2),
        section_id(3),
        sections,
        limits(),
    )
    .expect("knowledge snapshot")
}

#![allow(dead_code, reason = "shared deterministic integration-test builders")]

use peritus_memory::{
    ClaimType, Confidence, EvidenceSet, FeatureKey, FeatureWeight, Feedback, MemoryEvidence,
    MemoryId, MemoryMaterial, MemoryRecord, MemoryScope, MemoryTiming, Observation, RepositoryId,
    RetrievalFeature, RetrievalFeatures, ScopeKind, SourceEventSet, SourceProvenance,
    StateSnapshot,
};
use peritus_types::{EventId, EvidenceId, ProjectId, RevisionNumber, WorkspaceId};

pub fn memory_id(seed: u8) -> MemoryId {
    MemoryId::new([seed; 16]).expect("nonzero memory id")
}

pub fn feature_key(seed: u8) -> FeatureKey {
    FeatureKey::new([seed; 16]).expect("nonzero feature key")
}

pub fn observation(tick: u64) -> Observation {
    Observation::new(1, tick).expect("nonzero logical epoch")
}

pub fn revision(value: u64) -> RevisionNumber {
    RevisionNumber::new(value).expect("nonzero revision")
}

pub fn event(seed: u8) -> EventId {
    EventId::new([seed; 16]).expect("nonzero event id")
}

pub fn evidence(seed: u8) -> EvidenceId {
    EvidenceId::new([seed; 16]).expect("nonzero evidence id")
}

pub fn project_scope(seed: u8) -> MemoryScope {
    MemoryScope::new(
        ScopeKind::Project,
        Some(ProjectId::new([seed; 16]).expect("project")),
        None,
        None,
        None,
        None,
    )
    .expect("project scope")
}

pub fn repository_scope(seed: u8) -> MemoryScope {
    MemoryScope::new(
        ScopeKind::Repository,
        Some(ProjectId::new([seed; 16]).expect("project")),
        Some(WorkspaceId::new([seed.wrapping_add(1); 16]).expect("workspace")),
        Some(RepositoryId::new([seed.wrapping_add(2); 16]).expect("repository")),
        None,
        None,
    )
    .expect("repository scope")
}

#[derive(Clone, Debug)]
pub struct RecordOptions {
    pub seed: u8,
    pub content: Vec<u8>,
    pub tokens: u32,
    pub provenance: SourceProvenance,
    pub claim_type: ClaimType,
    pub confidence: u16,
    pub positive_feedback: u16,
    pub negative_feedback: u16,
    pub supporting: Vec<u8>,
    pub contradicting: Vec<u8>,
    pub reviewed_tick: Option<u64>,
    pub expiry_tick: Option<u64>,
    pub features: Vec<(u8, u8, u16)>,
    pub scope: MemoryScope,
}

impl RecordOptions {
    pub fn new(seed: u8) -> Self {
        Self {
            seed,
            content: format!("memory-{seed}").into_bytes(),
            tokens: 10,
            provenance: SourceProvenance::Repository,
            claim_type: ClaimType::Fact,
            confidence: 8_000,
            positive_feedback: 1,
            negative_feedback: 0,
            supporting: vec![seed.wrapping_add(40)],
            contradicting: Vec::new(),
            reviewed_tick: Some(2),
            expiry_tick: None,
            features: vec![(seed.wrapping_add(20), seed.wrapping_add(30), 10_000)],
            scope: project_scope(1),
        }
    }
}

pub fn make_record(options: RecordOptions) -> MemoryRecord {
    let digest = peritus_codec::sha256(&options.content);
    let material = MemoryMaterial::new(
        options.claim_type,
        digest,
        options.content,
        options.provenance,
        options.tokens,
    )
    .expect("material");
    let sources = SourceEventSet::new(vec![event(options.seed.wrapping_add(80))]).expect("source");
    let supporting = EvidenceSet::new(options.supporting.into_iter().map(evidence).collect())
        .expect("supporting");
    let contradicting = EvidenceSet::new(options.contradicting.into_iter().map(evidence).collect())
        .expect("contradicting");
    let bindings = MemoryEvidence::new(sources, supporting, contradicting).expect("bindings");
    let timing = MemoryTiming::new(
        observation(1),
        options.reviewed_tick.map(observation),
        options.expiry_tick.map(observation),
    )
    .expect("timing");
    let features = RetrievalFeatures::new(
        options
            .features
            .into_iter()
            .map(|(key, digest_seed, weight)| {
                RetrievalFeature::new(
                    feature_key(key),
                    peritus_types::Sha256Digest::new([digest_seed; 32]),
                    FeatureWeight::new(weight).expect("weight"),
                )
            })
            .collect(),
    )
    .expect("features");
    let state = StateSnapshot::active(
        Confidence::new(options.confidence).expect("confidence"),
        Feedback::new(options.positive_feedback, options.negative_feedback).expect("feedback"),
        RevisionNumber::first(),
    );
    MemoryRecord::new(
        memory_id(options.seed),
        options.scope,
        material,
        bindings,
        timing,
        features,
        state,
    )
    .expect("record")
}

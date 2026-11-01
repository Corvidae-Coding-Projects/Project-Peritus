//! Independent memory replay, lifecycle, filtering, and ranking investigation.

use std::collections::BTreeMap;

use peritus_memory::{
    BasisPoints, CandidateExplanation, ClaimType, ClaimTypeSet, Confidence, DeletionReason,
    EvidenceSet, ExclusionReason, FeatureKey, FeatureWeight, Feedback, FeedbackPolicy, MemoryError,
    MemoryErrorKind, MemoryEvidence, MemoryId, MemoryIndex, MemoryMaterial, MemoryRecord,
    MemoryScope, MemoryState, MemoryTiming, MemoryTombstone, Observation, QuarantineReason,
    RankingWeights, RepositoryId, RequiredFeatures, RetrievalFeature, RetrievalFeatures,
    RetrievalLimits, RetrievalPlan, RetrievalPolicy, RetrievalQuery, ScopeKind, ScopePolicy,
    SourceEventSet, SourceProvenance, StateSnapshot, retrieve,
};
use peritus_policy::ActorRole;
use peritus_role::RoleProfile;
use peritus_types::{
    ActorId, EventId, EvidenceId, ProjectId, RevisionNumber, Sha256Digest, WorkspaceId,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

/// Exact replayable initial records and operation history.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    pub initial: Vec<Record>,
    pub operations: Vec<Operation>,
}

/// Input-defined scope dimensions. Every scope always binds a project.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    pub kind: Kind,
    pub project: u8,
    pub workspace: Option<u8>,
    pub repository: Option<u8>,
    pub actor: Option<u8>,
    pub role: Option<Role>,
}

/// Declared retention scope kind, kept separate from its actual dimensions.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Project,
    Workspace,
    Repository,
    Actor,
    Role,
}

/// Independent role truth table, including excluded security roles.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum Role {
    Writer,
    Reviewer,
    Fixer,
    Evaluator,
    Evolver,
    GateRunner,
    Daemon,
}
impl Role {
    fn production(self) -> ActorRole {
        match self {
            Self::Writer => ActorRole::Writer,
            Self::Reviewer => ActorRole::Reviewer,
            Self::Fixer => ActorRole::Fixer,
            Self::Evaluator => ActorRole::Evaluator,
            Self::Evolver => ActorRole::EvolutionAgent,
            Self::GateRunner => ActorRole::GateRunner,
            Self::Daemon => ActorRole::DaemonService,
        }
    }
    fn excluded(self) -> bool {
        matches!(self, Self::Reviewer | Self::GateRunner | Self::Daemon)
    }
}

/// Canonical feature input. Equal keys with unequal values deliberately share index postings.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Feature {
    pub key: u8,
    pub value: u8,
    pub weight: u16,
}

/// Initial or imported later active snapshot. All content and retrieval fields are case data.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub scope: Scope,
    pub claim: u8,
    pub text: String,
    pub tokens: u16,
    pub features: Vec<Feature>,
    pub confidence: u16,
    pub positive: u8,
    pub negative: u8,
    pub supporting: u8,
    pub contradicting: u8,
    pub reviewed: bool,
    pub lifetime: Option<u8>,
}

/// Explicit attempt to use an old revision or observation, rather than skipping an operation.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Freshness {
    Fresh,
    SameRevision,
    SameObservation,
}

/// The six documented ranking components, or an exact weighted mixture of them.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Ranking {
    Scope,
    Relevance,
    Confidence,
    Evidence,
    Recency,
    Feedback,
    Balanced,
}
impl Ranking {
    fn weights(self) -> [u16; 6] {
        let mut values = [0; 6];
        match self {
            Self::Scope => values[0] = 10_000,
            Self::Relevance => values[1] = 10_000,
            Self::Confidence => values[2] = 10_000,
            Self::Evidence => values[3] = 10_000,
            Self::Recency => values[4] = 10_000,
            Self::Feedback => values[5] = 10_000,
            Self::Balanced => values = [1_000, 2_500, 2_500, 1_500, 1_500, 1_000],
        }
        values
    }
}

/// A capacity chosen independently from known eligible input costs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Budget {
    Absolute(u16),
    FirstCandidate(i8),
    AllCandidates(i8),
}

/// A fully specified retrieval policy/query and legal input order permutation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Query {
    pub scope: Scope,
    pub role: Role,
    pub broader: bool,
    pub accepted_claims: u8,
    pub features: Vec<Feature>,
    pub required_features: u8,
    pub minimum_confidence: u16,
    pub max_review_age: Option<u8>,
    pub negative_at: Option<u16>,
    pub contradiction_at: Option<u16>,
    pub ranking: Ranking,
    pub max_results: u8,
    pub budget: Budget,
    pub later_epoch: bool,
    pub now_delta: i16,
    pub reverse_input: bool,
}

/// Histories touch real immutable record transitions and canonical replay APIs.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    /// Imports a later checked active snapshot of an existing logical lineage.
    Publish {
        selector: u8,
        record: Record,
    },
    /// Replaces inert bytes by equal-length different bytes in an imported revision.
    ReplaceSameSize {
        selector: u8,
    },
    Review {
        selector: u8,
        confidence: u16,
        positive: u8,
        negative: u8,
        supporting: u8,
        contradicting: u8,
        freshness: Freshness,
    },
    Quarantine {
        selector: u8,
        freshness: Freshness,
    },
    Release {
        selector: u8,
        freshness: Freshness,
    },
    Expire {
        selector: u8,
        freshness: Freshness,
    },
    Supersede {
        selector: u8,
        freshness: Freshness,
    },
    Delete {
        selector: u8,
        freshness: Freshness,
    },
    Rebuild,
    RejectReplayOrder,
    RejectDuplicateIdentity,
    Retrieve(Query),
}

fn role_strategy() -> BoxedStrategy<Role> {
    proptest::sample::select(vec![
        Role::Writer,
        Role::Reviewer,
        Role::Fixer,
        Role::Evaluator,
        Role::Evolver,
        Role::GateRunner,
        Role::Daemon,
    ])
    .boxed()
}
fn scope_strategy() -> BoxedStrategy<Scope> {
    (0_u8..5, 0_u8..3, 0_u8..3, 0_u8..3, 0_u8..3, role_strategy())
        .prop_map(|(kind, project, workspace, repository, actor, role)| {
            let mut scope = project_scope(project);
            match kind {
                1 => {
                    scope.kind = Kind::Workspace;
                    scope.workspace = Some(workspace);
                }
                2 => {
                    scope.kind = Kind::Repository;
                    scope.repository = Some(repository);
                }
                3 => {
                    scope.kind = Kind::Actor;
                    scope.workspace = Some(workspace);
                    scope.actor = Some(actor);
                }
                4 => {
                    scope.kind = Kind::Role;
                    scope.workspace = Some(workspace);
                    scope.role = Some(role);
                }
                _ => {}
            }
            scope
        })
        .boxed()
}
fn features_strategy() -> BoxedStrategy<Vec<Feature>> {
    (any::<u8>(), any::<[u8; 4]>())
        .prop_map(|(mask, values)| {
            (0_u8..4)
                .filter(|key| mask & (1 << key) != 0)
                .map(|key| Feature {
                    key,
                    value: values[usize::from(key)] % 3,
                    weight: 1 + u16::from(values[usize::from(key)]) * 37,
                })
                .collect()
        })
        .boxed()
}
fn record_strategy() -> BoxedStrategy<Record> {
    let text = proptest::collection::vec(
        proptest::sample::select(vec!['a', 'b', ' ', 'é', '中', '🙂', '\u{301}', '"', '\\']),
        1..=24,
    )
    .prop_map(|chars| chars.into_iter().collect());
    (
        (scope_strategy(), 0_u8..7, text, 1_u16..=32, features_strategy()),
        (
            0_u16..=10_000,
            0_u8..=4,
            0_u8..=4,
            0_u8..=3,
            0_u8..=3,
            any::<bool>(),
            proptest::option::of(0_u8..=20),
        ),
    )
        .prop_map(
            |(
                (scope, claim, text, tokens, features),
                (confidence, positive, negative, supporting, contradicting, reviewed, lifetime),
            )| Record {
                scope,
                claim,
                text,
                tokens,
                features,
                confidence,
                positive,
                negative,
                supporting,
                contradicting,
                reviewed,
                lifetime,
            },
        )
        .boxed()
}
fn query_strategy() -> BoxedStrategy<Query> {
    let threshold =
        || proptest::option::of(proptest::sample::select(vec![0, 3_333, 5_000, 10_000]));
    let budget = prop_oneof![
        (1_u16..=128).prop_map(Budget::Absolute),
        (-1_i8..=1).prop_map(Budget::FirstCandidate),
        (-1_i8..=1).prop_map(Budget::AllCandidates),
    ];
    (
        (
            scope_strategy(),
            role_strategy(),
            any::<bool>(),
            1_u8..128,
            features_strategy(),
            0_u8..16,
        ),
        (0_u16..=10_000, proptest::option::of(0_u8..=20), threshold(), threshold()),
        (
            proptest::sample::select(vec![
                Ranking::Scope,
                Ranking::Relevance,
                Ranking::Confidence,
                Ranking::Evidence,
                Ranking::Recency,
                Ranking::Feedback,
                Ranking::Balanced,
            ]),
            1_u8..=8,
            budget,
            any::<bool>(),
            -32_i16..=32,
            any::<bool>(),
        ),
    )
        .prop_map(
            |(
                (scope, role, broader, accepted_claims, features, required_features),
                (minimum_confidence, max_review_age, negative_at, contradiction_at),
                (ranking, max_results, budget, later_epoch, now_delta, reverse_input),
            )| Query {
                scope,
                role,
                broader,
                accepted_claims,
                features,
                required_features,
                minimum_confidence,
                max_review_age,
                negative_at,
                contradiction_at,
                ranking,
                max_results,
                budget,
                later_epoch,
                now_delta,
                reverse_input,
            },
        )
        .boxed()
}

/// Small identity domains force overwrites, repeated lifecycle attempts, and shared features.
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let freshness = || {
        proptest::sample::select(vec![
            Freshness::Fresh,
            Freshness::Fresh,
            Freshness::SameRevision,
            Freshness::SameObservation,
        ])
    };
    let selected = || (any::<u8>(), freshness());
    let operation = prop_oneof![
        3 => (any::<u8>(), record_strategy()).prop_map(|(selector, record)| Operation::Publish { selector, record }),
        2 => any::<u8>().prop_map(|selector| Operation::ReplaceSameSize { selector }),
        3 => (any::<u8>(), 0_u16..=10_000, 0_u8..=4, 0_u8..=4, 0_u8..=3,
            0_u8..=3, freshness()).prop_map(|(selector, confidence, positive, negative,
                supporting, contradicting, freshness)| Operation::Review { selector, confidence,
                positive, negative, supporting, contradicting, freshness }),
        3 => selected().prop_map(|(selector, freshness)| Operation::Quarantine { selector, freshness }),
        3 => selected().prop_map(|(selector, freshness)| Operation::Release { selector, freshness }),
        1 => selected().prop_map(|(selector, freshness)| Operation::Expire { selector, freshness }),
        1 => selected().prop_map(|(selector, freshness)| Operation::Supersede { selector, freshness }),
        2 => selected().prop_map(|(selector, freshness)| Operation::Delete { selector, freshness }),
        2 => Just(Operation::Rebuild),
        1 => Just(Operation::RejectReplayOrder),
        1 => Just(Operation::RejectDuplicateIdentity),
        5 => query_strategy().prop_map(Operation::Retrieve),
    ];
    (
        proptest::collection::vec(record_strategy(), 1..=8),
        proptest::collection::vec(operation, 0..=max_steps.min(128)),
    )
        .prop_map(|(initial, operations)| Case { initial, operations })
        .boxed()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Life {
    Active,
    Quarantined,
    Expired,
    Superseded,
}
impl Life {
    fn production(self) -> MemoryState {
        match self {
            Self::Active => MemoryState::Active,
            Self::Quarantined => MemoryState::Quarantined,
            Self::Expired => MemoryState::Expired,
            Self::Superseded => MemoryState::Superseded,
        }
    }
}
#[derive(Clone, Debug)]
struct ModelRecord {
    input: Record,
    revision: u64,
    created: u64,
    reviewed: Option<u64>,
    changed: Option<u64>,
    life: Life,
}
impl ModelRecord {
    fn latest_tick(&self) -> u64 {
        self.created.max(self.reviewed.unwrap_or(0)).max(self.changed.unwrap_or(0))
    }
    fn expiry(&self) -> Option<u64> {
        self.input.lifetime.map(|age| self.created + u64::from(age))
    }
}
#[derive(Clone, Debug)]
struct Tombstone {
    slot: usize,
    revision: u64,
}
struct Machine {
    model: BTreeMap<usize, ModelRecord>,
    latest: BTreeMap<usize, MemoryRecord>,
    history: Vec<MemoryRecord>,
    tombstones: Vec<Tombstone>,
    actual_tombstones: Vec<MemoryTombstone>,
}

fn error(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}
fn bytes(index: usize) -> [u8; 16] {
    ((index as u128) + 1).to_be_bytes()
}
fn memory_id(slot: usize) -> Result<MemoryId, String> {
    MemoryId::new(bytes(slot)).map_err(error)
}
fn revision(value: u64) -> Result<RevisionNumber, String> {
    RevisionNumber::new(value).map_err(error)
}
fn observation(tick: u64) -> Result<Observation, String> {
    Observation::new(1, tick).map_err(error)
}
fn feature_key(key: u8) -> Result<FeatureKey, String> {
    FeatureKey::new(bytes(usize::from(key))).map_err(error)
}
fn feature_digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
fn claim(value: u8) -> Result<ClaimType, String> {
    match value {
        0 => Ok(ClaimType::Fact),
        1 => Ok(ClaimType::Preference),
        2 => Ok(ClaimType::Procedure),
        3 => Ok(ClaimType::Outcome),
        4 => Ok(ClaimType::Warning),
        5 => Ok(ClaimType::Constraint),
        6 => Ok(ClaimType::Hypothesis),
        _ => Err("invalid replay claim".into()),
    }
}
fn actual_scope(scope: &Scope) -> Result<MemoryScope, String> {
    let project = Some(ProjectId::new(bytes(usize::from(scope.project))).map_err(error)?);
    let workspace = scope
        .workspace
        .map(|value| WorkspaceId::new(bytes(usize::from(value))))
        .transpose()
        .map_err(error)?;
    let repository = scope
        .repository
        .map(|value| RepositoryId::new(bytes(usize::from(value))))
        .transpose()
        .map_err(error)?;
    let actor = scope
        .actor
        .map(|value| ActorId::new(bytes(usize::from(value))))
        .transpose()
        .map_err(error)?;
    MemoryScope::new(
        match scope.kind {
            Kind::Project => ScopeKind::Project,
            Kind::Workspace => ScopeKind::Workspace,
            Kind::Repository => ScopeKind::Repository,
            Kind::Actor => ScopeKind::Actor,
            Kind::Role => ScopeKind::Role,
        },
        project,
        workspace,
        repository,
        actor,
        scope.role.map(Role::production),
    )
    .map_err(error)
}
fn actual_features(features: &[Feature]) -> Result<RetrievalFeatures, String> {
    RetrievalFeatures::new(
        features
            .iter()
            .map(|feature| {
                Ok(RetrievalFeature::new(
                    feature_key(feature.key)?,
                    feature_digest(feature.value),
                    FeatureWeight::new(feature.weight).map_err(error)?,
                ))
            })
            .collect::<Result<_, String>>()?,
    )
    .map_err(error)
}
fn evidence(count: u8, offset: usize) -> Result<EvidenceSet, String> {
    EvidenceSet::new(
        (0..usize::from(count))
            .map(|index| EvidenceId::new(bytes(offset + index)))
            .collect::<Result<_, _>>()
            .map_err(error)?,
    )
    .map_err(error)
}
fn make_record(slot: usize, model: &ModelRecord) -> Result<MemoryRecord, String> {
    let input = &model.input;
    MemoryRecord::new(
        memory_id(slot)?,
        actual_scope(&input.scope)?,
        MemoryMaterial::new(
            claim(input.claim)?,
            peritus_codec::sha256(input.text.as_bytes()),
            input.text.as_bytes().to_vec(),
            SourceProvenance::Tool,
            u32::from(input.tokens),
        )
        .map_err(error)?,
        MemoryEvidence::new(
            SourceEventSet::new(vec![EventId::new(bytes(0)).map_err(error)?]).map_err(error)?,
            evidence(input.supporting, 0)?,
            evidence(input.contradicting, 100)?,
        )
        .map_err(error)?,
        MemoryTiming::new(
            observation(model.created)?,
            model.reviewed.map(observation).transpose()?,
            model.expiry().map(observation).transpose()?,
        )
        .map_err(error)?,
        actual_features(&input.features)?,
        StateSnapshot::active(
            Confidence::new(input.confidence).map_err(error)?,
            Feedback::new(u16::from(input.positive), u16::from(input.negative)).map_err(error)?,
            revision(model.revision)?,
        ),
    )
    .map_err(error)
}
fn slot_of(id: MemoryId) -> Result<usize, String> {
    usize::try_from(
        u128::from_be_bytes(*id.as_bytes()).checked_sub(1).ok_or("zero memory identity")?,
    )
    .map_err(error)
}
fn bump(metrics: &mut crate::Metrics, key: &str, value: u64) {
    *metrics.entry(key.into()).or_default() += value;
}
fn kind<T>(actual: Result<T, MemoryError>, expected: MemoryErrorKind) -> Result<(), String> {
    match actual {
        Err(error) if error.kind() == expected => Ok(()),
        Err(error) => Err(format!("expected {expected:?}, got {error:?}")),
        Ok(_) => Err(format!("expected {expected:?}, operation succeeded")),
    }
}

impl Machine {
    fn new(inputs: &[Record]) -> Result<Self, String> {
        let mut result = Self {
            model: BTreeMap::new(),
            latest: BTreeMap::new(),
            history: Vec::new(),
            tombstones: Vec::new(),
            actual_tombstones: Vec::new(),
        };
        for (slot, input) in inputs.iter().enumerate() {
            result.publish(slot, input.clone(), 1, slot as u64 + 1)?;
        }
        Ok(result)
    }
    fn publish(&mut self, slot: usize, input: Record, rev: u64, now: u64) -> Result<(), String> {
        let model = ModelRecord {
            reviewed: input.reviewed.then_some(now),
            input,
            revision: rev,
            created: now,
            changed: None,
            life: Life::Active,
        };
        let actual = make_record(slot, &model)?;
        self.history.push(actual.clone());
        self.latest.insert(slot, actual);
        self.model.insert(slot, model);
        Ok(())
    }
    fn selected_slot(&self, selector: u8) -> Result<usize, String> {
        self.model
            .keys()
            .nth(usize::from(selector) % self.model.len().max(1))
            .copied()
            .ok_or("invalid replay: lifecycle operation requires an existing record".into())
    }
    fn deleted(&self, slot: usize, rev: u64) -> bool {
        self.tombstones.iter().any(|tombstone| tombstone.slot == slot && tombstone.revision >= rev)
    }
    fn expected_active(&self) -> BTreeMap<usize, ModelRecord> {
        self.model
            .iter()
            .filter(|(slot, record)| {
                record.life == Life::Active && !self.deleted(**slot, record.revision)
            })
            .map(|(slot, record)| (*slot, record.clone()))
            .collect()
    }
    fn replay_inputs(&self) -> (Vec<MemoryRecord>, Vec<MemoryTombstone>) {
        let mut records = self.history.clone();
        records.sort_by_key(|record| (record.id(), record.revision()));
        let mut tombstones = self.actual_tombstones.clone();
        tombstones
            .sort_by_key(|tombstone| (tombstone.memory_id(), tombstone.last_known_revision()));
        (records, tombstones)
    }
    fn rebuild(&self, metrics: &mut crate::Metrics) -> Result<MemoryIndex, String> {
        let (records, tombstones) = self.replay_inputs();
        let index = MemoryIndex::rebuild(records, tombstones.clone()).map_err(error)?;
        bump(metrics, "index_rebuilds", 1);
        let expected = self.expected_active();
        let actual_ids = index
            .active_records()
            .iter()
            .map(|record| slot_of(record.id()))
            .collect::<Result<Vec<_>, _>>()?;
        if actual_ids != expected.keys().copied().collect::<Vec<_>>() {
            return Err(format!(
                "latest-revision/tombstone replay mismatch: expected {:?}, got {actual_ids:?}",
                expected.keys()
            ));
        }
        for record in index.active_records() {
            check_record(record, &expected[&slot_of(record.id())?])?;
        }
        if index.tombstones() != tombstones {
            return Err("retained tombstones changed during replay".into());
        }
        // The canonical active view must not depend on obsolete record revisions.
        let latest = self.latest.values().cloned().collect();
        let compact = MemoryIndex::rebuild(latest, tombstones).map_err(error)?;
        if index != compact {
            return Err("full-history index differs from latest-only reconstruction".into());
        }
        check_postings(&index, &expected)?;
        bump(metrics, "active_records_observed", actual_ids.len() as u64);
        bump(metrics, "posting_checks", 3);
        Ok(index)
    }
}

fn check_record(actual: &MemoryRecord, expected: &ModelRecord) -> Result<(), String> {
    let input = &expected.input;
    if actual.revision().get() != expected.revision
        || actual.material().content() != input.text.as_bytes()
        || actual.material().estimated_tokens() != u32::from(input.tokens)
        || actual.scope() != &actual_scope(&input.scope)?
        || actual.material().claim_type() != claim(input.claim)?
        || actual.lifecycle().state() != expected.life.production()
        || actual.lifecycle().confidence().basis_points().get() != input.confidence
        || actual.lifecycle().feedback().positive() != u16::from(input.positive)
        || actual.lifecycle().feedback().negative() != u16::from(input.negative)
        || actual.evidence().supporting().values().len() != usize::from(input.supporting)
        || actual.evidence().contradicting().values().len() != usize::from(input.contradicting)
        || actual.timing().created().tick() != expected.created
        || actual.timing().reviewed().map(|value| value.tick()) != expected.reviewed
        || actual.timing().expires().map(|value| value.tick()) != expected.expiry()
        || actual.latest_observation().tick() != expected.latest_tick()
        || actual.features() != &actual_features(&input.features)?
        || !actual.material().quoted_evidence()
    {
        return Err(format!("record metadata/content changed: {actual:?}; expected {expected:?}"));
    }
    Ok(())
}

fn check_postings(
    index: &MemoryIndex,
    expected: &BTreeMap<usize, ModelRecord>,
) -> Result<(), String> {
    let mut scopes = BTreeMap::<MemoryScope, Vec<MemoryId>>::new();
    let mut claims = BTreeMap::<ClaimType, Vec<MemoryId>>::new();
    let mut features = BTreeMap::<FeatureKey, Vec<MemoryId>>::new();
    for (slot, record) in expected {
        let id = memory_id(*slot)?;
        scopes.entry(actual_scope(&record.input.scope)?).or_default().push(id);
        claims.entry(claim(record.input.claim)?).or_default().push(id);
        for feature in &record.input.features {
            features.entry(feature_key(feature.key)?).or_default().push(id);
        }
    }
    let actual_scopes = index
        .scope_postings()
        .iter()
        .map(|posting| (*posting.scope(), posting.memory_ids().to_vec()))
        .collect::<Vec<_>>();
    let actual_claims = index
        .claim_postings()
        .iter()
        .map(|posting| (posting.claim_type(), posting.memory_ids().to_vec()))
        .collect::<Vec<_>>();
    let actual_features = index
        .feature_postings()
        .iter()
        .map(|posting| (posting.key(), posting.memory_ids().to_vec()))
        .collect::<Vec<_>>();
    if actual_scopes != scopes.into_iter().collect::<Vec<_>>()
        || actual_claims != claims.into_iter().collect::<Vec<_>>()
        || actual_features != features.into_iter().collect::<Vec<_>>()
    {
        return Err("secondary index posting differs from independent active-record map".into());
    }
    Ok(())
}

fn compatible(record: &Scope, query: &Scope, broader: bool) -> bool {
    if !broader {
        return record == query;
    }
    record.project == query.project
        && record.workspace.is_none_or(|value| Some(value) == query.workspace)
        && record.repository.is_none_or(|value| Some(value) == query.repository)
        && record.actor.is_none_or(|value| Some(value) == query.actor)
        && record.role.is_none_or(|value| Some(value) == query.role)
}

fn exclusion(
    machine: &Machine,
    slot: usize,
    record: &ModelRecord,
    query: &Query,
    now: u64,
    indexed: bool,
) -> Option<ExclusionReason> {
    let input = &record.input;
    let epoch = if query.later_epoch { 2 } else { 1 };
    let negative =
        ratio(u64::from(input.negative), u64::from(input.positive) + u64::from(input.negative), 0);
    let contradiction = ratio(
        u64::from(input.contradicting),
        u64::from(input.supporting) + u64::from(input.contradicting),
        0,
    );
    let stale = query.max_review_age.is_some_and(|age| {
        record
            .reviewed
            .is_none_or(|reviewed| epoch != 1 || reviewed > now || now - reviewed > u64::from(age))
    });
    // A literal priority table expresses the documented first-reason contract; no production
    // filter, compatibility, role-policy, feature-lookup, or verification helper is called.
    [
        (!indexed && machine.deleted(slot, record.revision), ExclusionReason::Tombstoned),
        (!compatible(&input.scope, &query.scope, query.broader), ExclusionReason::ScopeMismatch),
        (query.role.excluded(), ExclusionReason::RolePolicy),
        (record.life == Life::Quarantined, ExclusionReason::Quarantined),
        (record.life == Life::Expired, ExclusionReason::Expired),
        (record.life == Life::Superseded, ExclusionReason::Superseded),
        (epoch == 1 && record.latest_tick() > now, ExclusionReason::FutureObservation),
        (
            record.expiry().is_some_and(|expiry| epoch > 1 || expiry <= now),
            ExclusionReason::ExpiryReached,
        ),
        (input.confidence < query.minimum_confidence, ExclusionReason::BelowConfidence),
        (query.accepted_claims & (1 << input.claim) == 0, ExclusionReason::UnsupportedClaim),
        (input.supporting == 0, ExclusionReason::UnsupportedEvidence),
        (
            (0..4).any(|key| {
                query.required_features & (1 << key) != 0
                    && !input.features.iter().any(|feature| feature.key == key)
            }),
            ExclusionReason::MissingRequiredFeature,
        ),
        (stale, ExclusionReason::StaleReview),
        (
            query.negative_at.is_some_and(|threshold| negative >= u64::from(threshold)),
            ExclusionReason::NegativeFeedback,
        ),
        (
            query.contradiction_at.is_some_and(|threshold| contradiction >= u64::from(threshold)),
            ExclusionReason::Contradiction,
        ),
    ]
    .into_iter()
    .find_map(|(matches, reason)| matches.then_some(reason))
}
fn ratio(part: u64, total: u64, empty: u64) -> u64 {
    (part * 10_000).checked_div(total).unwrap_or(empty)
}
fn score(record: &ModelRecord, query: &Query, now: u64) -> [u64; 7] {
    let input = &record.input;
    let scope = 2_000
        * (1 + u64::from(input.scope.workspace.is_some())
            + u64::from(input.scope.repository.is_some())
            + u64::from(input.scope.actor.is_some())
            + u64::from(input.scope.role.is_some()));
    let total: u64 = query.features.iter().map(|feature| u64::from(feature.weight)).sum();
    let matched: u64 = query
        .features
        .iter()
        .filter(|feature| {
            input
                .features
                .iter()
                .any(|candidate| candidate.key == feature.key && candidate.value == feature.value)
        })
        .map(|feature| u64::from(feature.weight))
        .sum();
    let support = u64::from(input.supporting);
    let contradiction = u64::from(input.contradicting);
    let positive = u64::from(input.positive);
    let negative = u64::from(input.negative);
    let age = now.saturating_sub(record.reviewed.unwrap_or(record.created));
    let components = [
        scope,
        ratio(matched, total, 0),
        u64::from(input.confidence),
        10_000 - ratio(contradiction, support + contradiction, 0),
        if query.later_epoch { 0 } else { 10_000 - age.min(10_000) },
        ratio(positive, positive + negative, 5_000),
    ];
    let weighted = components
        .iter()
        .zip(query.ranking.weights())
        .map(|(component, weight)| component * u64::from(weight))
        .sum::<u64>()
        / 10_000;
    [
        components[0],
        components[1],
        components[2],
        components[3],
        components[4],
        components[5],
        weighted,
    ]
}

#[derive(Clone, Debug)]
struct Expected {
    slot: usize,
    revision: u64,
    reason: Option<ExclusionReason>,
    score: Option<[u64; 7]>,
}
fn model_candidates(
    machine: &Machine,
    records: &BTreeMap<usize, ModelRecord>,
    query: &Query,
    now: u64,
    indexed: bool,
) -> Vec<Expected> {
    records
        .iter()
        .map(|(slot, record)| {
            let reason = exclusion(machine, *slot, record, query, now, indexed);
            Expected {
                slot: *slot,
                revision: record.revision,
                score: reason.is_none().then(|| score(record, query, now)),
                reason,
            }
        })
        .collect()
}
fn capacity(query: &Query, records: &BTreeMap<usize, ModelRecord>, eligible: &[Expected]) -> u32 {
    let mut ranked: Vec<_> =
        eligible.iter().filter(|candidate| candidate.reason.is_none()).collect();
    ranked.sort_by_key(|candidate| {
        (std::cmp::Reverse(candidate.score.map(|value| value[6]).unwrap_or(0)), candidate.slot)
    });
    let (base, delta) = match query.budget {
        Budget::Absolute(value) => return u32::from(value),
        Budget::FirstCandidate(delta) => (
            ranked.first().map_or(1, |candidate| u32::from(records[&candidate.slot].input.tokens)),
            delta,
        ),
        Budget::AllCandidates(delta) => (
            ranked
                .iter()
                .map(|candidate| u32::from(records[&candidate.slot].input.tokens))
                .sum::<u32>()
                .max(1),
            delta,
        ),
    };
    if delta < 0 {
        base.saturating_sub(u32::from(delta.unsigned_abs()))
    } else {
        base.saturating_add(delta as u32)
    }
}
fn expected_plan(
    mut candidates: Vec<Expected>,
    records: &BTreeMap<usize, ModelRecord>,
    budget: u32,
    count: usize,
) -> (Vec<Expected>, Vec<usize>, u32) {
    let mut order: Vec<_> = candidates
        .iter()
        .enumerate()
        .filter_map(|(index, candidate)| {
            candidate.score.map(|value| (index, value[6], candidate.slot))
        })
        .collect();
    order.sort_by_key(|(_, total, slot)| (std::cmp::Reverse(*total), *slot));
    let mut selected = Vec::new();
    let mut used = 0;
    for (index, _, slot) in order {
        let tokens = u32::from(records[&slot].input.tokens);
        if selected.len() == count {
            candidates[index].reason = Some(ExclusionReason::ResultLimit);
        } else if used + tokens > budget {
            candidates[index].reason = Some(ExclusionReason::TokenBudget);
        } else {
            selected.push(slot);
            used += tokens;
        }
    }
    (candidates, selected, used)
}

fn actual_policy(query: &Query) -> Result<RetrievalPolicy, String> {
    let weights = query.ranking.weights().map(|value| BasisPoints::new(value).map_err(error));
    let [scope, relevance, confidence, evidence, recency, feedback] = weights;
    Ok(RetrievalPolicy::new(
        RetrievalLimits::new(
            u16::from(query.max_results),
            Confidence::new(query.minimum_confidence).map_err(error)?,
            query.max_review_age.map(u64::from),
        )
        .map_err(error)?,
        ClaimTypeSet::new(
            (0..7)
                .filter(|value| query.accepted_claims & (1 << value) != 0)
                .map(claim)
                .collect::<Result<_, _>>()?,
        )
        .map_err(error)?,
        RankingWeights::new(scope?, relevance?, confidence?, evidence?, recency?, feedback?)
            .map_err(error)?,
        FeedbackPolicy::new(
            query.negative_at.map(BasisPoints::new).transpose().map_err(error)?,
            query.contradiction_at.map(BasisPoints::new).transpose().map_err(error)?,
        ),
        if query.broader { ScopePolicy::IncludeBroader } else { ScopePolicy::Exact },
    ))
}
fn actual_query(
    query: &Query,
    now: u64,
    budget: u32,
) -> Result<Result<RetrievalQuery, MemoryError>, String> {
    let scope = actual_scope(&query.scope)?;
    let features = actual_features(&query.features)?;
    let required = RequiredFeatures::new(
        (0..4)
            .filter(|key| query.required_features & (1 << key) != 0)
            .map(feature_key)
            .collect::<Result<_, _>>()?,
    )
    .map_err(error)?;
    Ok(RetrievalQuery::new(
        scope,
        RoleProfile::for_actor_role(query.role.production()),
        Observation::new(if query.later_epoch { 2 } else { 1 }, now).map_err(error)?,
        features,
        required,
        budget,
    ))
}
fn score_values(score: peritus_memory::RankScore) -> [u64; 7] {
    [
        score.scope(),
        score.relevance(),
        score.confidence(),
        score.evidence(),
        score.recency(),
        score.feedback(),
        score.total(),
    ]
    .map(|value| u64::from(value.get()))
}
fn check_plan(
    plan: &RetrievalPlan,
    records: &BTreeMap<usize, ModelRecord>,
    expected: &[Expected],
    selected: &[usize],
    used: u32,
    budget: u32,
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    let actual_selected = plan
        .selected()
        .iter()
        .map(|candidate| slot_of(candidate.id()))
        .collect::<Result<Vec<_>, _>>()?;
    if actual_selected != selected
        || plan.used_tokens() != used
        || plan.token_budget() != budget
        || plan.remaining_tokens() != budget - used
    {
        return Err(format!(
            "retrieval selection/accounting mismatch: expected {selected:?}/{used}, got {actual_selected:?}/{}",
            plan.used_tokens()
        ));
    }
    for candidate in plan.selected() {
        let slot = slot_of(candidate.id())?;
        let input = &records[&slot].input;
        let expected_score = expected
            .iter()
            .find(|value| value.slot == slot)
            .and_then(|value| value.score)
            .ok_or("missing expected selected score")?;
        if candidate.revision().get() != records[&slot].revision
            || candidate.material().content() != input.text.as_bytes()
            || candidate.estimated_tokens() != u32::from(input.tokens)
            || candidate.scope() != &actual_scope(&input.scope)?
            || !candidate.quoted_evidence()
            || score_values(candidate.score()) != expected_score
        {
            return Err(format!(
                "selected memory {slot} lost content/revision/score/quote semantics"
            ));
        }
    }
    if plan.explanations().len() != expected.len() {
        return Err("retrieval explanation count is incomplete".into());
    }
    for (actual, expected) in plan.explanations().iter().zip(expected) {
        if slot_of(actual.id())? != expected.slot {
            return Err("retrieval explanations not in exact stable-ID order".into());
        }
        match (actual, expected.reason) {
            (CandidateExplanation::Selected(_, revision, score), None)
                if revision.get() == expected.revision
                    && Some(score_values(*score)) == expected.score => {}
            (CandidateExplanation::Excluded(excluded), Some(reason))
                if excluded.reason() == reason
                    && excluded.revision().get() == expected.revision
                    && excluded.score().map(score_values) == expected.score =>
            {
                bump(metrics, &format!("excluded_{reason:?}"), 1);
            }
            _ => {
                return Err(format!(
                    "explanation differs for {}: {actual:?}; expected {expected:?}",
                    expected.slot
                ));
            }
        }
    }
    bump(metrics, "selected_candidates", selected.len() as u64);
    bump(
        metrics,
        "scored_candidates",
        expected.iter().filter(|candidate| candidate.score.is_some()).count() as u64,
    );
    bump(metrics, "explained_candidates", expected.len() as u64);
    Ok(())
}

fn probe(
    machine: &Machine,
    query: &Query,
    clock: u64,
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    let now = if query.now_delta < 0 {
        clock.saturating_sub(u64::from(query.now_delta.unsigned_abs()))
    } else {
        clock + query.now_delta as u64
    };
    let inputs = model_candidates(machine, &machine.model, query, now, false);
    let budget = capacity(query, &machine.model, &inputs);
    let policy = actual_policy(query)?;
    let actual_query = actual_query(query, now, budget)?;
    if budget == 0 {
        kind(actual_query, MemoryErrorKind::InvalidBound)?;
        bump(metrics, "zero_budget_rejections", 1);
        return Ok(());
    }
    let actual_query = actual_query.map_err(error)?;
    let mut records: Vec<_> = machine.latest.values().cloned().collect();
    let mut tombstones = machine.actual_tombstones.clone();
    if query.reverse_input {
        records.reverse();
        tombstones.reverse();
        if records.len() > 1 || tombstones.len() > 1 {
            bump(metrics, "reordered_retrievals", 1);
        }
    }
    let result = retrieve(&records, &tombstones, &policy, &actual_query).map_err(error)?;
    let (expected, selected, used) =
        expected_plan(inputs, &machine.model, budget, usize::from(query.max_results));
    check_plan(&result, &machine.model, &expected, &selected, used, budget, metrics)?;
    let index = machine.rebuild(metrics)?;
    let active = machine.expected_active();
    let (expected_active, selected_active, used_active) = expected_plan(
        model_candidates(machine, &active, query, now, true),
        &active,
        budget,
        usize::from(query.max_results),
    );
    let indexed = index.retrieve(&policy, &actual_query).map_err(error)?;
    check_plan(
        &indexed,
        &active,
        &expected_active,
        &selected_active,
        used_active,
        budget,
        metrics,
    )?;
    if indexed.selected() != result.selected() {
        return Err(
            "index retrieval and full latest-snapshot scan select different memories".into()
        );
    }
    bump(metrics, "retrievals", 2);
    bump(metrics, &format!("queries_role_{:?}", query.role), 1);
    bump(metrics, &format!("queries_ranking_{:?}", query.ranking), 1);
    bump(
        metrics,
        if query.role.excluded() { "excluded_role_queries" } else { "evidence_role_queries" },
        1,
    );
    bump(metrics, if query.broader { "broader_scope_queries" } else { "exact_scope_queries" }, 1);
    bump(
        metrics,
        match query.budget {
            Budget::FirstCandidate(-1) | Budget::AllCandidates(-1) => "below_budget_queries",
            Budget::FirstCandidate(0) | Budget::AllCandidates(0) => "exact_budget_queries",
            Budget::FirstCandidate(1) | Budget::AllCandidates(1) => "above_budget_queries",
            _ => "absolute_budget_queries",
        },
        1,
    );
    Ok(())
}

fn transition(
    machine: &mut Machine,
    operation: &Operation,
    rev: u64,
    clock: u64,
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    let (selector, freshness) = match operation {
        Operation::Review { selector, freshness, .. }
        | Operation::Quarantine { selector, freshness }
        | Operation::Release { selector, freshness }
        | Operation::Expire { selector, freshness }
        | Operation::Supersede { selector, freshness }
        | Operation::Delete { selector, freshness } => (*selector, *freshness),
        _ => return Err("non-transition passed to transition runner".into()),
    };
    let slot = machine.selected_slot(selector)?;
    let old = machine.model[&slot].clone();
    let actual = &machine.latest[&slot];
    let rev = if matches!(freshness, Freshness::SameRevision) { old.revision } else { rev };
    let tick =
        if matches!(freshness, Freshness::SameObservation) { old.latest_tick() } else { clock };
    let invalid_state = match operation {
        Operation::Review { .. } => matches!(old.life, Life::Expired | Life::Superseded),
        Operation::Release { .. } => old.life != Life::Quarantined,
        Operation::Delete { .. } => false,
        _ => old.life != Life::Active,
    };
    let expected_error = if invalid_state {
        Some(MemoryErrorKind::InvalidTransition)
    } else if rev <= old.revision {
        Some(MemoryErrorKind::InvalidRevision)
    } else if tick <= old.latest_tick() {
        Some(if matches!(operation, Operation::Release { .. }) {
            MemoryErrorKind::ReleaseRequiresReview
        } else {
            MemoryErrorKind::StaleObservation
        })
    } else {
        None
    };
    let mut next = old.clone();
    next.revision = rev;
    let actual_result = match operation {
        Operation::Review { confidence, positive, negative, supporting, contradicting, .. } => {
            next.input.confidence = *confidence;
            next.input.positive = *positive;
            next.input.negative = *negative;
            next.input.supporting = *supporting;
            next.input.contradicting = *contradicting;
            next.reviewed = Some(tick);
            bump(metrics, "review_attempts", 1);
            actual.review(
                revision(rev)?,
                observation(tick)?,
                Confidence::new(*confidence).map_err(error)?,
                evidence(*supporting, 0)?,
                evidence(*contradicting, 100)?,
                Feedback::new(u16::from(*positive), u16::from(*negative)).map_err(error)?,
            )
        }
        Operation::Quarantine { .. } => {
            next.life = Life::Quarantined;
            next.changed = Some(tick);
            bump(metrics, "quarantine_attempts", 1);
            actual.quarantine(revision(rev)?, observation(tick)?, QuarantineReason::ManualReview)
        }
        Operation::Release { .. } => {
            next.life = Life::Active;
            next.changed = None;
            next.reviewed = Some(tick);
            bump(metrics, "release_attempts", 1);
            actual.release(
                revision(rev)?,
                observation(tick)?,
                Confidence::new(old.input.confidence).map_err(error)?,
                Feedback::new(u16::from(old.input.positive), u16::from(old.input.negative))
                    .map_err(error)?,
            )
        }
        Operation::Expire { .. } => {
            next.life = Life::Expired;
            next.changed = Some(tick);
            bump(metrics, "expire_attempts", 1);
            actual.expire(revision(rev)?, observation(tick)?)
        }
        Operation::Supersede { .. } => {
            next.life = Life::Superseded;
            next.changed = Some(tick);
            bump(metrics, "supersede_attempts", 1);
            actual.supersede(revision(rev)?, observation(tick)?, memory_id(slot + 100)?)
        }
        Operation::Delete { .. } => {
            bump(metrics, "delete_attempts", 1);
            let result =
                actual.forget(revision(rev)?, observation(tick)?, DeletionReason::UserRequest);
            if let Some(expected) = expected_error {
                kind(result, expected)?;
                bump(metrics, "rejected_lifecycle_transitions", 1);
                bump(metrics, &format!("rejected_{expected:?}"), 1);
            } else {
                let tombstone = result.map_err(error)?;
                if tombstone.memory_id() != memory_id(slot)?
                    || tombstone.last_known_revision().get() != rev
                    || tombstone.deletion_observation().tick() != tick
                {
                    return Err(
                        "deletion tombstone lost exact identity/revision/observation".into()
                    );
                }
                machine.tombstones.push(Tombstone { slot, revision: rev });
                machine.actual_tombstones.push(tombstone);
                bump(metrics, "successful_deletions", 1);
            }
            return Ok(());
        }
        _ => return Err("unknown lifecycle operation".into()),
    };
    if let Some(expected) = expected_error {
        kind(actual_result, expected)?;
        bump(metrics, "rejected_lifecycle_transitions", 1);
        bump(metrics, &format!("rejected_{expected:?}"), 1);
    } else {
        let actual = actual_result.map_err(error)?;
        check_record(&actual, &next)?;
        machine.history.push(actual.clone());
        machine.latest.insert(slot, actual);
        machine.model.insert(slot, next);
        bump(metrics, "successful_lifecycle_transitions", 1);
        bump(
            metrics,
            match operation {
                Operation::Review { .. } => "successful_reviews",
                Operation::Quarantine { .. } => "successful_quarantines",
                Operation::Release { .. } => "successful_releases",
                Operation::Expire { .. } => "successful_expiries",
                Operation::Supersede { .. } => "successful_supersessions",
                _ => return Err("unknown successful transition".into()),
            },
            1,
        );
    }
    Ok(())
}

/// Checks a replay history against independent lineage, postings, filter, and arithmetic models.
pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    if case.initial.len() > 8 || case.operations.len() > 128 {
        return Err("memory replay exceeds pilot limits".into());
    }
    let mut machine = Machine::new(&case.initial)?;
    let mut metrics = crate::Metrics::new();
    bump(&mut metrics, "cases", 1);
    bump(&mut metrics, "initial_records", case.initial.len() as u64);
    machine.rebuild(&mut metrics)?;
    probe(&machine, &default_query(), 100, &mut metrics)?;
    for (step, operation) in case.operations.iter().enumerate() {
        let rev = step as u64 + 2;
        let clock = step as u64 + 100;
        let result = match operation {
            Operation::Publish { selector, record } => {
                let slot =
                    if machine.model.is_empty() { 0 } else { machine.selected_slot(*selector)? };
                machine.publish(slot, record.clone(), rev, clock)?;
                bump(&mut metrics, "published_revisions", 1);
                Ok(())
            }
            Operation::ReplaceSameSize { selector } => {
                let slot = machine.selected_slot(*selector)?;
                let mut record = machine.model[&slot].input.clone();
                let old = record.text.as_bytes();
                record.text = if old.iter().all(|byte| *byte == b'x') {
                    "y".repeat(old.len())
                } else {
                    "x".repeat(old.len())
                };
                machine.publish(slot, record, rev, clock)?;
                bump(&mut metrics, "different_content_equal_bytes_replacements", 1);
                Ok(())
            }
            Operation::Retrieve(query) => probe(&machine, query, clock, &mut metrics),
            Operation::Rebuild => machine.rebuild(&mut metrics).map(|_| ()),
            Operation::RejectReplayOrder => {
                let (mut records, tombstones) = machine.replay_inputs();
                if records.len() < 2 {
                    let slot = machine.selected_slot(0)?;
                    let mut record = machine.model[&slot].input.clone();
                    record.text.push('!');
                    machine.publish(slot, record, rev, clock)?;
                    (records, _) = machine.replay_inputs();
                }
                records.reverse();
                kind(
                    MemoryIndex::rebuild(records, tombstones),
                    MemoryErrorKind::NonCanonicalOrder,
                )?;
                bump(&mut metrics, "noncanonical_replay_rejections", 1);
                Ok(())
            }
            Operation::RejectDuplicateIdentity => {
                let slot = machine.selected_slot(0)?;
                let record = machine.latest[&slot].clone();
                kind(
                    MemoryIndex::rebuild(vec![record.clone(), record.clone()], vec![]),
                    MemoryErrorKind::ConflictingRevision,
                )?;
                let query = actual_query(&default_query(), clock, 100)?.map_err(error)?;
                kind(
                    retrieve(
                        &[record.clone(), record],
                        &[],
                        &actual_policy(&default_query())?,
                        &query,
                    ),
                    MemoryErrorKind::DuplicateValue,
                )?;
                bump(&mut metrics, "duplicate_identity_rejections", 2);
                Ok(())
            }
            _ => transition(&mut machine, operation, rev, clock, &mut metrics),
        };
        result.map_err(|detail| format!("step {step}, {operation:?}: {detail}"))?;
        // Complete maps/postings are checked after every change; retained old snapshots remain
        // in production replay inputs and cannot choose operands or define expected state.
        if !matches!(operation, Operation::Retrieve(_) | Operation::Rebuild) {
            machine.rebuild(&mut metrics)?;
        }
        bump(&mut metrics, "operations", 1);
    }
    for delta in [-1, 0, 1] {
        let mut query = default_query();
        query.budget = Budget::AllCandidates(delta);
        query.reverse_input = delta == 0;
        probe(&machine, &query, case.operations.len() as u64 + 100, &mut metrics)?;
    }
    Ok(metrics)
}

fn project_scope(project: u8) -> Scope {
    Scope {
        kind: Kind::Project,
        project,
        workspace: None,
        repository: None,
        actor: None,
        role: None,
    }
}
fn default_record() -> Record {
    Record {
        scope: project_scope(0),
        claim: 0,
        text: "inert memory".into(),
        tokens: 5,
        features: vec![Feature { key: 0, value: 0, weight: 1 }],
        confidence: 8_000,
        positive: 1,
        negative: 0,
        supporting: 1,
        contradicting: 0,
        reviewed: true,
        lifetime: None,
    }
}
fn default_query() -> Query {
    Query {
        scope: project_scope(0),
        role: Role::Writer,
        broader: true,
        accepted_claims: 127,
        features: vec![Feature { key: 0, value: 0, weight: 1 }],
        required_features: 0,
        minimum_confidence: 0,
        max_review_age: None,
        negative_at: None,
        contradiction_at: None,
        ranking: Ranking::Balanced,
        max_results: 8,
        budget: Budget::Absolute(1000),
        later_epoch: false,
        now_delta: 0,
        reverse_input: false,
    }
}

/// A later quarantine must replace the earlier active snapshot in every posting and read.
pub fn calibration_case() -> Case {
    Case {
        initial: vec![default_record()],
        operations: vec![
            Operation::Quarantine { selector: 0, freshness: Freshness::Fresh },
            Operation::Rebuild,
            Operation::Retrieve(default_query()),
        ],
    }
}

/// Fixed cases cover each public transition, filters, rank components, exact capacities and ties.
pub fn examples() -> Vec<Case> {
    let mut lifecycle = calibration_case();
    lifecycle.operations.extend([
        Operation::Release { selector: 0, freshness: Freshness::SameObservation },
        Operation::Release { selector: 0, freshness: Freshness::Fresh },
        Operation::Review {
            selector: 0,
            confidence: 9_000,
            positive: 0,
            negative: 1,
            supporting: 2,
            contradicting: 1,
            freshness: Freshness::SameRevision,
        },
        Operation::Review {
            selector: 0,
            confidence: 9_000,
            positive: 0,
            negative: 1,
            supporting: 2,
            contradicting: 1,
            freshness: Freshness::Fresh,
        },
        Operation::ReplaceSameSize { selector: 0 },
        Operation::Delete { selector: 0, freshness: Freshness::Fresh },
        Operation::Rebuild,
        Operation::Publish { selector: 0, record: default_record() },
        Operation::Expire { selector: 0, freshness: Freshness::Fresh },
        Operation::Review {
            selector: 0,
            confidence: 9_000,
            positive: 1,
            negative: 0,
            supporting: 1,
            contradicting: 0,
            freshness: Freshness::Fresh,
        },
        Operation::Publish { selector: 0, record: default_record() },
        Operation::Supersede { selector: 0, freshness: Freshness::Fresh },
        Operation::RejectReplayOrder,
        Operation::RejectDuplicateIdentity,
    ]);
    let mut cases =
        vec![Case { initial: vec![], operations: vec![] }, calibration_case(), lifecycle];
    for role in [
        Role::Writer,
        Role::Reviewer,
        Role::Fixer,
        Role::Evaluator,
        Role::Evolver,
        Role::GateRunner,
        Role::Daemon,
    ] {
        let mut query = default_query();
        query.role = role;
        cases.push(Case {
            initial: vec![default_record()],
            operations: vec![Operation::Retrieve(query)],
        });
    }
    for ranking in [
        Ranking::Scope,
        Ranking::Relevance,
        Ranking::Confidence,
        Ranking::Evidence,
        Ranking::Recency,
        Ranking::Feedback,
        Ranking::Balanced,
    ] {
        let mut first = default_record();
        first.features[0].value = 1;
        first.negative = 2;
        first.positive = 1;
        let mut second = default_record();
        second.confidence = 5_000;
        second.contradicting = 1;
        let mut query = default_query();
        query.ranking = ranking;
        query.max_results = 1;
        query.reverse_input = true;
        let operations = [-1, 0, 1]
            .into_iter()
            .map(|delta| {
                let mut query = query.clone();
                query.budget = Budget::FirstCandidate(delta);
                Operation::Retrieve(query)
            })
            .collect();
        cases.push(Case { initial: vec![first, second, default_record()], operations });
    }
    // Isolate each ordered filter using one otherwise eligible record and one changed field.
    for filter in 0..11 {
        let mut record = default_record();
        let mut query = default_query();
        match filter {
            0 => query.scope = project_scope(1),
            1 => {
                record.reviewed = false;
                query.max_review_age = Some(0);
            }
            2 => {
                record.supporting = 0;
            }
            3 => {
                query.required_features = 2;
            }
            4 => {
                query.minimum_confidence = 8_001;
            }
            5 => {
                query.accepted_claims = 2;
            }
            6 => {
                record.positive = 0;
                record.negative = 1;
                query.negative_at = Some(10_000);
            }
            7 => {
                record.contradicting = 1;
                query.contradiction_at = Some(5_000);
            }
            8 => {
                query.now_delta = -100;
            }
            9 => {
                record.lifetime = Some(1);
            }
            _ => {
                query.max_review_age = Some(20);
                query.later_epoch = true;
            }
        }
        cases.push(Case { initial: vec![record], operations: vec![Operation::Retrieve(query)] });
    }
    for broader in [false, true] {
        let mut query = default_query();
        query.scope.kind = Kind::Workspace;
        query.scope.workspace = Some(0);
        query.broader = broader;
        let mut exact = default_record();
        exact.scope = query.scope.clone();
        cases.push(Case {
            initial: vec![default_record(), exact],
            operations: vec![Operation::Retrieve(query)],
        });
    }
    cases
}

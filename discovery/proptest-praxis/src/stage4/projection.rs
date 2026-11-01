//! Production catalog folds and SQLite shadow publication against a list-derived semantic view.

use super::journal::{Event, Fixture, bump, bytes, error, expected_head, hash, revision};
use crate::Metrics;
use peritus_journal::{
    AggregateId, AggregateKey, AggregateKind, AppendRequest, EventDraft, ExactFrame,
    HeadExpectation, IntegrityExport, SqliteJournal, SqliteJournalOptions,
};
use peritus_projection::{
    ActiveGeneration, CatalogGeneration, FoldContext, JournalCatalogProjection,
    JournalCatalogState, Projection, ProjectionError, ProjectionErrorKind, ProjectionIdentity,
    ProjectionName, ProjectionSchema, ProjectionState, ProjectionStore, ProjectionVersion,
    RepairAction, RepairReason, StoreOptions, rebuild_from_genesis, replay_from_genesis,
};
use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};
use proptest::prelude::*;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::{num::NonZeroU64, time::Duration};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    steps: Vec<Op>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum Expectation {
    Current,
    Absent,
    Previous,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
enum Op {
    Append { aggregate: u8, value: u8, external: bool },
    Capture(u8),
    Publish { slot: u8, expectation: Expectation },
    Refresh,
    Repeat,
    CorruptPayload,
    FailedSwap,
    Nondeterministic,
    SchemaChange,
    Reopen,
    InvalidReplay { schema: bool },
    EmptyReplay,
}
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let operation = prop_oneof![
        7=>(0_u8..4,any::<u8>(),any::<bool>()).prop_map(|(aggregate,value,external)|Op::Append{aggregate,value,external}),
        4=>(0_u8..4).prop_map(Op::Capture),
        5=>(0_u8..4,proptest::sample::select(vec![Expectation::Current,Expectation::Absent,Expectation::Previous])).prop_map(|(slot,expectation)|Op::Publish{slot,expectation}),
        4=>Just(Op::Refresh),3=>Just(Op::Repeat),2=>Just(Op::CorruptPayload),2=>Just(Op::FailedSwap),
        2=>Just(Op::Nondeterministic),2=>Just(Op::SchemaChange),2=>Just(Op::Reopen),
        2=>any::<bool>().prop_map(|schema|Op::InvalidReplay{schema}),
        1=>Just(Op::EmptyReplay),
    ];
    proptest::collection::vec(operation, 0..=max_steps.min(64))
        .prop_map(|steps| Case { steps })
        .boxed()
}

#[derive(Clone)]
struct Expected {
    payload: Vec<u8>,
    head: Sha256Digest,
    count: u64,
    aggregates: usize,
    families: [u64; 2],
    schema: ProjectionSchema,
}
impl Expected {
    fn from_events(events: &[Event], schema: ProjectionSchema) -> Result<Self, String> {
        let heads = (0..4)
            .filter_map(|id| events.iter().rev().find(|event| event.aggregate == id))
            .collect::<Vec<_>>();
        let count = u64::try_from(events.len()).map_err(error)?;
        let mut payload = b"peritus-journal-catalog-v1\0".to_vec();
        payload.extend(count.to_be_bytes());
        payload.extend(u64::try_from(heads.len()).map_err(error)?.to_be_bytes());
        for head in &heads {
            payload.extend(8_u16.to_be_bytes());
            payload.extend(bytes(u64::from(head.aggregate)));
            payload.extend(head.position.to_be_bytes());
            payload.extend(head.sequence.to_be_bytes());
            payload.extend(head.family.to_be_bytes());
            payload.extend(head.event_hash.as_bytes());
            payload.extend(revision(head.aggregate).as_bytes());
        }
        let families = [
            u64::try_from(events.iter().filter(|event| event.family == 1).count())
                .map_err(error)?,
            u64::try_from(events.iter().filter(|event| event.family == 2).count())
                .map_err(error)?,
        ];
        payload.extend(
            u64::try_from(families.iter().filter(|count| **count > 0).count())
                .map_err(error)?
                .to_be_bytes(),
        );
        for (index, count) in families.iter().enumerate() {
            if *count > 0 {
                payload.extend(u16::try_from(index + 1).map_err(error)?.to_be_bytes());
                payload.extend(count.to_be_bytes());
            }
        }
        Ok(Self {
            payload,
            head: expected_head(events)?,
            count,
            aggregates: heads.len(),
            families,
            schema,
        })
    }
    fn invariant(&self) -> Sha256Digest {
        hash(&[b"peritus-journal-catalog-invariants-v1\0".as_slice(), &self.payload].concat())
    }
    fn check_replay(
        &self,
        state: &JournalCatalogState,
        payload: &[u8],
        checkpoint: &peritus_projection::Checkpoint,
        invariant: Sha256Digest,
        count: u64,
    ) -> Result<(), String> {
        if state.event_count() != self.count
            || state.aggregate_count() != self.aggregates
            || state.family_count(1) != self.families[0]
            || state.family_count(2) != self.families[1]
            || payload != self.payload
            || count != self.count
            || checkpoint.last_position() != self.count
            || checkpoint.journal_head_digest() != self.head
            || checkpoint.schema() != &self.schema
            || checkpoint.payload_digest() != hash(&self.payload)
            || invariant != self.invariant()
        {
            return Err(
                "catalog fold/checkpoint differs from independent list-derived semantic payload"
                    .into(),
            );
        }
        Ok(())
    }
}
#[derive(Clone)]
struct Snapshot {
    export: IntegrityExport,
    expected: Expected,
}
#[derive(Clone)]
struct Published {
    expected: Expected,
    generation: u64,
    corrupt: bool,
}
struct Harness {
    store: Option<ProjectionStore>,
    journal: Fixture,
    projection: JournalCatalogProjection,
    saved: [Snapshot; 4],
    active: Option<Published>,
    generations: u64,
    metrics: Metrics,
}
impl Harness {
    fn new() -> Result<Self, String> {
        let mut journal = Fixture::new()?;
        journal.fresh(0, 1, false, false, 2)?;
        journal.fresh(1, 2, false, true, 2)?;
        let projection = JournalCatalogProjection::new().map_err(error)?;
        let export = journal.journal()?.integrity_export().map_err(error)?;
        let expected = Expected::from_events(&journal.events, projection.schema().clone())?;
        let snapshot = Snapshot { export, expected };
        let saved = std::array::from_fn(|_| snapshot.clone());
        let store = ProjectionStore::open(&journal.path, StoreOptions::new(Duration::ZERO))
            .map_err(error)?;
        Ok(Self {
            store: Some(store),
            journal,
            projection,
            saved,
            active: None,
            generations: 0,
            metrics: Metrics::new(),
        })
    }
    fn store(&mut self) -> Result<&mut ProjectionStore, String> {
        self.store.as_mut().ok_or_else(|| "projection store closed".into())
    }
    fn snapshot(&mut self, schema: ProjectionSchema) -> Result<Snapshot, String> {
        let export = self.journal.journal()?.integrity_export().map_err(error)?;
        let expected = Expected::from_events(&self.journal.events, schema)?;
        Ok(Snapshot { export, expected })
    }
    fn capture(&mut self, slot: u8) -> Result<(), String> {
        let snapshot = self.snapshot(self.projection.schema().clone())?;
        let replay = replay_from_genesis(&self.projection, &snapshot.export).map_err(error)?;
        snapshot.expected.check_replay(
            replay.state(),
            replay.payload(),
            replay.checkpoint(),
            replay.invariant_digest(),
            replay.record_count(),
        )?;
        self.saved[usize::from(slot) % 4] = snapshot;
        bump(&mut self.metrics, "independent_catalog_folds");
        Ok(())
    }
    fn expected_active(
        &self,
        expectation: Expectation,
    ) -> Result<Option<CatalogGeneration>, String> {
        let current = self.active.as_ref().map(|row| row.generation);
        let number = match expectation {
            Expectation::Current => current,
            Expectation::Absent => None,
            Expectation::Previous => Some(current.unwrap_or(2).saturating_sub(1).max(1)),
        };
        number
            .map(|number| {
                NonZeroU64::new(number)
                    .map(CatalogGeneration::new)
                    .ok_or_else(|| "invalid model generation".into())
            })
            .transpose()
    }
    fn publish(&mut self, snapshot: &Snapshot, expectation: Expectation) -> Result<(), String> {
        let projection =
            Alternate::new(self.projection.clone(), snapshot.expected.schema.clone(), false);
        let candidate = rebuild_from_genesis(&projection, &snapshot.export).map_err(error)?;
        snapshot.expected.check_replay(
            &candidate.state().base,
            candidate.payload(),
            candidate.checkpoint(),
            candidate.invariant_digest(),
            candidate.record_count(),
        )?;
        let expected = self.expected_active(expectation)?;
        let matches =
            expected.map(CatalogGeneration::get) == self.active.as_ref().map(|row| row.generation);
        let same = self.active.as_ref().is_some_and(|row| {
            !row.corrupt
                && row.expected.count == snapshot.expected.count
                && row.expected.head == snapshot.expected.head
                && row.expected.schema == snapshot.expected.schema
        });
        let observation = self.journal.journal()?.observe_replay().map_err(error)?;
        let actual = self.store()?.install_shadow(&candidate, expected);
        if !matches {
            require_projection_error(actual, ProjectionErrorKind::Conflict)?;
            bump(&mut self.metrics, "stale_generation_cas_rejections");
        } else {
            let generation = actual.map_err(error)?.get();
            let expected_number = if same {
                self.active.as_ref().ok_or("active missing")?.generation
            } else {
                self.generations + 1
            };
            if generation != expected_number {
                return Err(
                    "shadow generation allocation/reuse disagrees with publication model".into()
                );
            }
            if same {
                bump(&mut self.metrics, "identical_generation_reuses");
            } else {
                self.generations += 1;
                bump(&mut self.metrics, "new_shadow_generations");
            }
            let current = self
                .journal
                .journal()?
                .replay_observation_is_current(&observation)
                .map_err(error)?;
            if current != same {
                return Err("projection commit did not invalidate only changed external journal observations".into());
            }
            self.active =
                Some(Published { expected: snapshot.expected.clone(), generation, corrupt: false });
        }
        self.verify()
    }
    fn refresh(&mut self) -> Result<(), String> {
        let snapshot = self.snapshot(self.projection.schema().clone())?;
        self.publish(&snapshot, Expectation::Current)
    }
    fn verify(&mut self) -> Result<(), String> {
        let schema = self.projection.schema().clone();
        let actual = self.store()?.load_active(&schema).map_err(error)?;
        if self.store()?.generation_count(&schema).map_err(error)? != self.generations {
            return Err("failed or repeated swap leaked/deleted durable generations".into());
        }
        match (&self.active, actual.as_ref()) {
            (None, None) => {}
            (Some(expected), Some(actual)) => verify_active(actual, expected)?,
            _ => return Err("active pointer differs from publication model".into()),
        }
        let export = self.journal.journal()?.integrity_export().map_err(error)?;
        let expected = match &self.active {
            None => RepairAction::RebuildFromGenesis(RepairReason::Missing),
            Some(row) if row.expected.schema != schema => {
                RepairAction::RebuildFromGenesis(RepairReason::SchemaChanged)
            }
            Some(row) if row.corrupt => {
                RepairAction::RebuildFromGenesis(RepairReason::PayloadCorrupt)
            }
            Some(row)
                if row.expected.count
                    != u64::try_from(self.journal.events.len()).map_err(error)? =>
            {
                RepairAction::RebuildFromGenesis(RepairReason::PositionChanged)
            }
            Some(row) if row.expected.head != expected_head(&self.journal.events)? => {
                RepairAction::RebuildFromGenesis(RepairReason::JournalHeadChanged)
            }
            Some(row) => RepairAction::Reuse(CatalogGeneration::new(
                NonZeroU64::new(row.generation).ok_or("zero model generation")?,
            )),
        };
        if self.store()?.plan_startup(&schema, export.report()).map_err(error)? != expected {
            return Err(
                "startup reuse/repair reason differs from independent journal/catalog relationship"
                    .into(),
            );
        }
        bump(&mut self.metrics, "startup_relationship_checks");
        Ok(())
    }
    fn corrupt(&mut self) -> Result<(), String> {
        self.refresh()?;
        let connection = Connection::open(&self.journal.path).map_err(error)?;
        connection.execute("UPDATE peritus_projection_generations SET payload=?1 WHERE projection_name='journal-catalog' AND generation=(SELECT active_generation FROM peritus_projection_catalog WHERE projection_name='journal-catalog')",params![b"owned corrupt payload".as_slice()]).map_err(error)?;
        self.active.as_mut().ok_or("active missing")?.corrupt = true;
        bump(&mut self.metrics, "actual_payload_corruptions");
        self.verify()
    }
    fn failed_swap(&mut self) -> Result<(), String> {
        self.refresh()?;
        self.journal.fresh(2, 73, false, false, 2)?;
        let snapshot = self.snapshot(self.projection.schema().clone())?;
        let candidate = rebuild_from_genesis(&self.projection, &snapshot.export).map_err(error)?;
        let connection = Connection::open(&self.journal.path).map_err(error)?;
        connection.execute_batch("CREATE TRIGGER praxis_projection_abort BEFORE UPDATE ON peritus_projection_catalog BEGIN SELECT RAISE(ABORT,'owned after-shadow-insert failure'); END").map_err(error)?;
        let expected = self.expected_active(Expectation::Current)?;
        let result = self.store()?.install_shadow(&candidate, expected);
        connection.execute_batch("DROP TRIGGER praxis_projection_abort").map_err(error)?;
        match result {
            Err(failure)
                if failure.kind() == ProjectionErrorKind::Storage
                    && failure.operation() == "activate projection generation"
                    && std::error::Error::source(&failure).is_some_and(|source| {
                        source.to_string().contains("owned after-shadow-insert failure")
                    }) => {}
            other => {
                return Err(format!(
                    "projection fault did not reach pointer publication: {other:?}"
                ));
            }
        }
        self.verify()?;
        bump(&mut self.metrics, "failure_after_shadow_insert");
        bump(&mut self.metrics, "continued_after_failed_swap");
        Ok(())
    }
    fn nondeterministic(&mut self) -> Result<(), String> {
        self.refresh()?;
        let snapshot = self.snapshot(self.projection.schema().clone())?;
        let altered =
            Alternate::new(self.projection.clone(), self.projection.schema().clone(), true);
        let candidate = rebuild_from_genesis(&altered, &snapshot.export).map_err(error)?;
        if candidate.payload() == snapshot.expected.payload {
            return Err("nondeterminism fixture did not alter semantic payload".into());
        }
        let expected = self.expected_active(Expectation::Current)?;
        let result = self.store()?.install_shadow(&candidate, expected);
        require_projection_error(result, ProjectionErrorKind::FoldInvariant)?;
        bump(&mut self.metrics, "same_binding_nondeterminism_rejections");
        self.verify()
    }
    fn schema_change(&mut self) -> Result<(), String> {
        let identity = ProjectionIdentity::new(
            ProjectionName::new("journal-catalog").map_err(error)?,
            ProjectionVersion::new(NonZeroU64::new(1).ok_or("zero version")?),
        );
        let schema = ProjectionSchema::new(identity, b"independently selected changed descriptor")
            .map_err(error)?;
        let snapshot = self.snapshot(schema)?;
        self.publish(&snapshot, Expectation::Current)?;
        bump(&mut self.metrics, "schema_bound_publications");
        Ok(())
    }
    fn reopen(&mut self) -> Result<(), String> {
        drop(self.store.take());
        self.journal.reopen()?;
        self.store = Some(
            ProjectionStore::open(&self.journal.path, StoreOptions::new(Duration::ZERO))
                .map_err(error)?,
        );
        bump(&mut self.metrics, "actual_projection_reopens");
        self.verify()
    }
}

fn verify_active(actual: &ActiveGeneration, expected: &Published) -> Result<(), String> {
    let payload = if expected.corrupt {
        b"owned corrupt payload".as_slice()
    } else {
        &expected.expected.payload
    };
    if actual.generation().get() != expected.generation
        || actual.payload() != payload
        || actual.payload_is_valid() == expected.corrupt
        || actual.record_count() != expected.expected.count
        || actual.checkpoint().last_position() != expected.expected.count
        || actual.checkpoint().journal_head_digest() != expected.expected.head
        || actual.checkpoint().payload_digest() != hash(&expected.expected.payload)
        || actual.checkpoint().schema() != &expected.expected.schema
        || actual.invariant_digest() != expected.expected.invariant()
    {
        return Err("published active generation differs from independent semantic snapshot".into());
    }
    Ok(())
}
fn require_projection_error<T: std::fmt::Debug>(
    result: Result<T, ProjectionError>,
    expected: ProjectionErrorKind,
) -> Result<(), String> {
    match result {
        Err(e) if e.kind() == expected => Ok(()),
        other => Err(format!("expected {expected:?}; observed {other:?}")),
    }
}

struct AlteredState {
    base: JournalCatalogState,
    alter: bool,
}
impl PartialEq for AlteredState {
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base && self.alter == other.alter
    }
}
impl Eq for AlteredState {}
impl ProjectionState for AlteredState {
    fn encode(&self) -> Vec<u8> {
        let mut payload = self.base.encode();
        if self.alter {
            payload.push(17);
        }
        payload
    }
    fn validate(&self) -> Result<(), ProjectionError> {
        self.base.validate()
    }
    fn invariant_digest(&self) -> Sha256Digest {
        if self.alter { hash(&self.encode()) } else { self.base.invariant_digest() }
    }
}
struct Alternate {
    base: JournalCatalogProjection,
    schema: ProjectionSchema,
    alter: bool,
}
impl Alternate {
    fn new(base: JournalCatalogProjection, schema: ProjectionSchema, alter: bool) -> Self {
        Self { base, schema, alter }
    }
}
impl Projection for Alternate {
    type State = AlteredState;
    fn schema(&self) -> &ProjectionSchema {
        &self.schema
    }
    fn genesis(&self) -> Self::State {
        AlteredState { base: self.base.genesis(), alter: self.alter }
    }
    fn fold(&self, state: &mut Self::State, input: FoldContext<'_>) -> Result<(), ProjectionError> {
        self.base.fold(&mut state.base, input)
    }
}

fn invalid_replay(schema: bool) -> Result<(), String> {
    let directory = tempfile::tempdir().map_err(error)?;
    let mut journal = SqliteJournal::open(
        directory.path().join("invalid.sqlite"),
        super::journal::store_id()?,
        SqliteJournalOptions { busy_timeout: Duration::ZERO },
    )
    .map_err(error)?;
    let aggregate =
        AggregateKey::new(AggregateKind::Trace, AggregateId::new([61; 16]).map_err(error)?);
    let raw = peritus_codec::encode_frame(
        if schema { 1 } else { 65530 },
        if schema { 999 } else { 1 },
        b"opaque accepted journal bytes",
        peritus_codec::CodecLimits::PRODUCTION,
    )
    .map_err(error)?;
    let event = EventDraft::new(
        aggregate,
        EventSequence::new(1).map_err(error)?,
        EventId::new([62; 16]).map_err(error)?,
        None,
        ExactFrame::new(raw).map_err(error)?,
        Sha256Digest::new([63; 32]),
        vec![],
    )
    .map_err(error)?;
    let plan = AppendRequest::new(
        journal.store_id(),
        CommandId::new([64; 16]).map_err(error)?,
        hash(b"invalid projection input"),
        vec![HeadExpectation::Absent(aggregate)],
        vec![event],
        vec![],
        vec![],
        None,
        None,
        vec![],
    )
    .plan()
    .map_err(error)?;
    journal.append(plan).map_err(error)?;
    let export = journal.integrity_export().map_err(error)?;
    require_projection_error(
        replay_from_genesis(&JournalCatalogProjection::new().map_err(error)?, &export),
        if schema {
            ProjectionErrorKind::UnsupportedSchema
        } else {
            ProjectionErrorKind::UnsupportedFamily
        },
    )
}

fn empty_replay() -> Result<(), String> {
    let mut fixture = Fixture::new()?;
    let projection = JournalCatalogProjection::new().map_err(error)?;
    let export = fixture.journal()?.integrity_export().map_err(error)?;
    let expected = Expected::from_events(&[], projection.schema().clone())?;
    let candidate = rebuild_from_genesis(&projection, &export).map_err(error)?;
    expected.check_replay(
        candidate.state(),
        candidate.payload(),
        candidate.checkpoint(),
        candidate.invariant_digest(),
        candidate.record_count(),
    )?;
    let mut store =
        ProjectionStore::open(&fixture.path, StoreOptions::new(Duration::ZERO)).map_err(error)?;
    if store.plan_startup(projection.schema(), export.report()).map_err(error)?
        != RepairAction::RebuildFromGenesis(RepairReason::Missing)
    {
        return Err("empty initial projection did not require publication".into());
    }
    let installed = store.install_shadow(&candidate, None).map_err(error)?;
    if installed.get() != 1 {
        return Err("empty catalog initial generation differs".into());
    }
    drop(store);
    let store =
        ProjectionStore::open(&fixture.path, StoreOptions::new(Duration::ZERO)).map_err(error)?;
    let active = store
        .load_active(projection.schema())
        .map_err(error)?
        .ok_or("empty published catalog lost on reopen")?;
    verify_active(&active, &Published { expected, generation: 1, corrupt: false })?;
    if store.generation_count(projection.schema()).map_err(error)? != 1
        || store.plan_startup(projection.schema(), export.report()).map_err(error)?
            != RepairAction::Reuse(installed)
    {
        return Err("empty reopened catalog is not exactly reusable".into());
    }
    Ok(())
}

pub fn check(case: &Case) -> Result<Metrics, String> {
    if case.steps.len() > 64 {
        return Err("setup-incomplete: projection history exceeds bounded campaign".into());
    }
    let mut harness = Harness::new()?;
    for (index, operation) in case.steps.iter().enumerate() {
        let result = match *operation {
            Op::Append { aggregate, value, external } => {
                harness.journal.fresh(aggregate % 4, value, false, external, 2)
            }
            Op::Capture(slot) => harness.capture(slot),
            Op::Publish { slot, expectation } => {
                let snapshot = harness.saved[usize::from(slot) % 4].clone();
                harness.publish(&snapshot, expectation)
            }
            Op::Refresh => harness.refresh(),
            Op::Repeat => {
                harness.refresh()?;
                harness.refresh()
            }
            Op::CorruptPayload => harness.corrupt(),
            Op::FailedSwap => harness.failed_swap(),
            Op::Nondeterministic => harness.nondeterministic(),
            Op::SchemaChange => harness.schema_change(),
            Op::Reopen => harness.reopen(),
            Op::InvalidReplay { schema } => {
                invalid_replay(schema)?;
                bump(
                    &mut harness.metrics,
                    if schema {
                        "unsupported_schema_rejections"
                    } else {
                        "unsupported_family_rejections"
                    },
                );
                Ok(())
            }
            Op::EmptyReplay => {
                empty_replay()?;
                bump(&mut harness.metrics, "empty_genesis_rebuilds");
                bump(&mut harness.metrics, "empty_catalog_publication_reopens");
                Ok(())
            }
        };
        result.map_err(|e| format!("{e}; step {index} {operation:?}"))?;
        harness.verify()?;
        bump(&mut harness.metrics, "operations");
    }
    for (key, count) in harness.journal.metrics {
        *harness.metrics.entry(format!("journal_{key}")).or_default() += count;
    }
    Ok(harness.metrics)
}
pub fn calibration() -> Case {
    Case { steps: vec![Op::Capture(0), Op::Refresh] }
}
pub fn examples() -> Vec<Case> {
    vec![
        calibration(),
        Case { steps: vec![Op::EmptyReplay, Op::Refresh] },
        Case {
            steps: vec![
                Op::Refresh,
                Op::Repeat,
                Op::Append { aggregate: 1, value: 7, external: false },
                Op::Capture(1),
                Op::Publish { slot: 0, expectation: Expectation::Absent },
                Op::Publish { slot: 1, expectation: Expectation::Current },
                Op::Publish { slot: 0, expectation: Expectation::Current },
                Op::Reopen,
                Op::Refresh,
            ],
        },
        Case {
            steps: vec![
                Op::Refresh,
                Op::CorruptPayload,
                Op::Reopen,
                Op::Refresh,
                Op::FailedSwap,
                Op::Refresh,
                Op::Nondeterministic,
                Op::SchemaChange,
                Op::Reopen,
                Op::Refresh,
            ],
        },
        Case {
            steps: vec![
                Op::InvalidReplay { schema: false },
                Op::InvalidReplay { schema: true },
                Op::Refresh,
            ],
        },
    ]
}

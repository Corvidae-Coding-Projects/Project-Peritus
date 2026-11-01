//! Actual C0 SQLite append, observation, immutable history, and delivery histories.

use crate::Metrics;
use peritus_journal::{
    AggregateHead, AggregateId, AggregateKey, AggregateKind, AppendPlan, AppendRequest,
    CommandResolution, CommittedBatch, CommittedRecord, EventDraft, ExactFrame, HeadExpectation,
    JournalError, JournalErrorKind, OutboxDraft, OutboxId, OutboxState, ReplayObservation,
    SqliteJournal, SqliteJournalOptions, StateInstall, StoreId,
};
use peritus_types::{CommandId, EventId, EventSequence, Sha256Digest};
use proptest::prelude::*;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::Duration,
};

const STATE_NAMESPACE: u16 = 701;

pub(super) fn error(value: impl std::fmt::Debug) -> String {
    format!("{value:?}")
}
pub(super) fn bump(metrics: &mut Metrics, key: &str) {
    *metrics.entry(key.into()).or_default() += 1;
}
pub(super) fn bytes(number: u64) -> [u8; 16] {
    (u128::from(number) + 1).to_be_bytes()
}
pub(super) fn hash(bytes: &[u8]) -> Sha256Digest {
    peritus_codec::sha256(bytes)
}
pub(super) fn store_id() -> Result<StoreId, String> {
    StoreId::new([41; 16]).map_err(error)
}
pub(super) fn key(id: u8) -> Result<AggregateKey, String> {
    Ok(AggregateKey::new(
        AggregateKind::Trace,
        AggregateId::new(bytes(u64::from(id))).map_err(error)?,
    ))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    steps: Vec<Op>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum Rejection {
    Head,
    State,
    WrongStore,
    Locked,
    Transaction,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum Fence {
    Current,
    Old,
    Future,
    Zero,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
enum Op {
    Append { aggregate: u8, value: u8, outbox: bool, external: bool, maximum: u8 },
    Retry { selector: u8, conflict: bool },
    Observe(u8),
    ObservedAppend(u8),
    Reject { aggregate: u8, kind: Rejection },
    InvalidPlan,
    ExternalCommit,
    Reopen,
    Claim { advance: u8, duration: u8 },
    Acknowledge { selector: u8, fence: Fence },
    TypedOwners,
    AtomicBatch,
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let operation = prop_oneof![
        8 => (0_u8..4, any::<u8>(), any::<bool>(), any::<bool>(), 1_u8..=3)
            .prop_map(|(aggregate,value,outbox,external,maximum)| Op::Append { aggregate,value,outbox,external,maximum }),
        4 => (any::<u8>(),any::<bool>()).prop_map(|(selector,conflict)| Op::Retry {selector,conflict}),
        4 => (0_u8..4).prop_map(Op::Observe),
        4 => (0_u8..4).prop_map(Op::ObservedAppend),
        4 => (0_u8..4,proptest::sample::select(vec![Rejection::Head,Rejection::State,Rejection::WrongStore,Rejection::Locked,Rejection::Transaction]))
            .prop_map(|(aggregate,kind)| Op::Reject {aggregate,kind}),
        1 => Just(Op::InvalidPlan), 2 => Just(Op::ExternalCommit), 2 => Just(Op::Reopen),
        4 => (0_u8..=3,1_u8..=4).prop_map(|(advance,duration)| Op::Claim {advance,duration}),
        3 => (any::<u8>(),proptest::sample::select(vec![Fence::Current,Fence::Old,Fence::Future,Fence::Zero]))
            .prop_map(|(selector,fence)| Op::Acknowledge {selector,fence}),
        1 => Just(Op::TypedOwners),
        1 => Just(Op::AtomicBatch),
    ];
    proptest::collection::vec(operation, 0..=max_steps.min(96))
        .prop_map(|steps| Case { steps })
        .boxed()
}

#[derive(Clone, Debug)]
pub(super) struct Event {
    pub aggregate: u8,
    pub position: u64,
    pub sequence: u64,
    pub number: u64,
    pub previous: Option<u64>,
    pub previous_hash: Sha256Digest,
    pub event_hash: Sha256Digest,
    pub frame: Vec<u8>,
    pub family: u16,
    pub value: u8,
}
#[derive(Clone)]
struct Packet {
    event: Event,
    head: Option<AggregateHead>,
    state: Option<u64>,
    outbox: bool,
    maximum: u8,
    digest: Sha256Digest,
}
impl Packet {
    fn request(&self, wrong_store: bool, conflict: bool) -> Result<AppendRequest, String> {
        let event = &self.event;
        let aggregate = key(event.aggregate)?;
        let draft = EventDraft::new(
            aggregate,
            EventSequence::new(event.sequence).map_err(error)?,
            EventId::new(bytes(event.number)).map_err(error)?,
            event.previous.map(|id| EventId::new(bytes(id)).map_err(error)).transpose()?,
            ExactFrame::new(event.frame.clone()).map_err(error)?,
            revision(event.aggregate),
            Vec::new(),
        )
        .map_err(error)?;
        let installs = vec![
            StateInstall::new(
                STATE_NAMESPACE,
                vec![event.aggregate],
                self.state,
                self.state.map_or(1, |prior| prior + 1),
                vec![event.value],
            )
            .map_err(error)?,
        ];
        let outbox = if self.outbox {
            vec![
                OutboxDraft::new(
                    OutboxId::new(bytes(event.number)).map_err(error)?,
                    "fixture://delivery".into(),
                    vec![event.value],
                    u16::from(self.maximum),
                )
                .map_err(error)?,
            ]
        } else {
            vec![]
        };
        let digest =
            if conflict { hash(b"intentionally conflicting request") } else { self.digest };
        Ok(AppendRequest::new(
            if wrong_store { StoreId::new([42; 16]).map_err(error)? } else { store_id()? },
            CommandId::new(bytes(event.number)).map_err(error)?,
            digest,
            vec![self.head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present)],
            vec![draft],
            installs,
            vec![],
            None,
            None,
            outbox,
        ))
    }
    fn plan(&self) -> Result<AppendPlan, String> {
        self.request(false, false)?.plan().map_err(error)
    }
}

pub(super) fn revision(aggregate: u8) -> Sha256Digest {
    Sha256Digest::new([aggregate + 1; 32])
}
pub(super) fn expected_head(events: &[Event]) -> Result<Sha256Digest, String> {
    let heads = (0..4)
        .filter_map(|aggregate| events.iter().rev().find(|e| e.aggregate == aggregate))
        .collect::<Vec<_>>();
    let mut input = b"peritus.journal.head.v1\0".to_vec();
    input.extend([41; 16]);
    input.extend(u64::try_from(events.len()).map_err(error)?.to_be_bytes());
    input.extend(u32::try_from(heads.len()).map_err(error)?.to_be_bytes());
    for head in heads {
        input.extend(8_u16.to_be_bytes());
        input.extend(bytes(u64::from(head.aggregate)));
        input.extend(head.sequence.to_be_bytes());
        input.extend(head.event_hash.as_bytes());
    }
    Ok(hash(&input))
}
fn frame(family: u16, value: u8) -> Vec<u8> {
    let mut result = b"PRTS".to_vec();
    for field in [1_u16, family, 1, 0] {
        result.extend(field.to_be_bytes());
    }
    result.extend(1_u32.to_be_bytes());
    result.push(value);
    result
}
fn event_hash(event: &Event) -> Sha256Digest {
    let previous = event.previous.map_or([0; 16], bytes);
    let input = [
        b"peritus.journal.event.v1\0".as_slice(),
        &8_u16.to_be_bytes(),
        &bytes(u64::from(event.aggregate)),
        &event.sequence.to_be_bytes(),
        &bytes(event.number),
        &[u8::from(event.previous.is_some())],
        &previous,
        event.previous_hash.as_bytes(),
        &bytes(event.number),
        hash(&event.frame).as_bytes(),
        revision(event.aggregate).as_bytes(),
        &0_u32.to_be_bytes(),
    ]
    .concat();
    hash(&input)
}
fn batch_hash(packet: &Packet) -> Sha256Digest {
    hash(
        &[
            b"peritus.journal.batch.v1\0".as_slice(),
            &[41; 16],
            &bytes(packet.event.number),
            packet.digest.as_bytes(),
            &1_u32.to_be_bytes(),
            packet.event.event_hash.as_bytes(),
            &0_u32.to_be_bytes(),
        ]
        .concat(),
    )
}

#[derive(Clone, Copy)]
struct ObservationVersion {
    instance: u64,
    attempt: u64,
    external: u64,
}
struct Marker {
    observation: ReplayObservation,
    version: ObservationVersion,
}
#[derive(Clone, Debug)]
struct Delivery {
    number: u64,
    position: u64,
    value: u8,
    maximum: u16,
    attempts: u16,
    state: OutboxState,
    fence: Option<u64>,
    until: Option<u64>,
}

/// Real disposable journal and independent committed event list; shared only as a consumer adapter.
pub(super) struct Fixture {
    journal: Option<SqliteJournal>,
    peer: Option<SqliteJournal>,
    pub path: PathBuf,
    _directory: tempfile::TempDir,
    pub events: Vec<Event>,
    commands: Vec<Packet>,
    deliveries: BTreeMap<u64, Delivery>,
    markers: [Option<Marker>; 4],
    next: u64,
    instance: u64,
    attempt: u64,
    external: u64,
    now: u64,
    pub metrics: Metrics,
}
impl Fixture {
    pub fn new() -> Result<Self, String> {
        let directory = tempfile::tempdir().map_err(error)?;
        let path = directory.path().join("journal.sqlite");
        let journal = open(&path)?;
        let peer = open(&path)?;
        Ok(Self {
            journal: Some(journal),
            peer: Some(peer),
            path,
            _directory: directory,
            events: vec![],
            commands: vec![],
            deliveries: BTreeMap::new(),
            markers: std::array::from_fn(|_| None),
            next: 0,
            instance: 0,
            attempt: 0,
            external: 0,
            now: 1,
            metrics: Metrics::new(),
        })
    }
    pub fn journal(&mut self) -> Result<&mut SqliteJournal, String> {
        self.journal.as_mut().ok_or_else(|| "journal closed".into())
    }
    fn version(&self) -> ObservationVersion {
        ObservationVersion {
            instance: self.instance,
            attempt: self.attempt,
            external: self.external,
        }
    }
    fn current(&self, version: ObservationVersion) -> bool {
        version.instance == self.instance
            && version.attempt == self.attempt
            && version.external == self.external
    }
    fn packet(
        &mut self,
        aggregate: u8,
        value: u8,
        outbox: bool,
        maximum: u8,
    ) -> Result<Packet, String> {
        self.next += 1;
        let prior = self.events.iter().rev().find(|e| e.aggregate == aggregate).cloned();
        let mut event = Event {
            aggregate,
            position: u64::try_from(self.events.len()).map_err(error)? + 1,
            sequence: prior.as_ref().map_or(1, |e| e.sequence + 1),
            number: self.next,
            previous: prior.as_ref().map(|e| e.number),
            previous_hash: prior.as_ref().map_or(Sha256Digest::new([0; 32]), |e| e.event_hash),
            event_hash: Sha256Digest::new([0; 32]),
            frame: frame(1 + u16::from(value % 2), value),
            family: 1 + u16::from(value % 2),
            value,
        };
        event.event_hash = event_hash(&event);
        let head = self.journal()?.head(key(aggregate)?).map_err(error)?;
        // The opaque handle cannot be constructed publicly; independently verify it before use.
        verify_head(head, self.events.iter().rev().find(|e| e.aggregate == aggregate))?;
        let digest = hash(
            &[
                b"praxis-journal-command".as_slice(),
                &self.next.to_be_bytes(),
                &[aggregate, value, u8::from(outbox), maximum],
            ]
            .concat(),
        );
        Ok(Packet {
            event,
            head,
            state: prior.as_ref().map(|e| e.sequence),
            outbox,
            maximum,
            digest,
        })
    }
    fn commit_model(&mut self, packet: Packet, batch: &CommittedBatch) -> Result<(), String> {
        verify_batch(batch, &packet)?;
        if packet.outbox {
            self.deliveries.insert(
                packet.event.number,
                Delivery {
                    number: packet.event.number,
                    position: packet.event.position,
                    value: packet.event.value,
                    maximum: u16::from(packet.maximum),
                    attempts: 0,
                    state: OutboxState::Pending,
                    fence: None,
                    until: None,
                },
            );
        }
        self.events.push(packet.event.clone());
        self.commands.push(packet);
        Ok(())
    }
    pub fn fresh(
        &mut self,
        aggregate: u8,
        value: u8,
        outbox: bool,
        external: bool,
        maximum: u8,
    ) -> Result<(), String> {
        let packet = self.packet(aggregate, value, outbox, maximum)?;
        let plan = packet.plan()?;
        let batch = if external {
            self.peer.as_mut().ok_or("peer closed")?.append(plan).map_err(error)?
        } else {
            self.attempt += 1;
            self.journal()?.append(plan).map_err(error)?
        };
        if external {
            self.external += 1;
            bump(&mut self.metrics, "external_journal_commits");
        }
        self.commit_model(packet, &batch)?;
        bump(&mut self.metrics, "committed_appends");
        self.verify()
    }
    pub fn reopen(&mut self) -> Result<(), String> {
        drop(self.journal.take());
        drop(self.peer.take());
        self.journal = Some(open(&self.path)?);
        self.peer = Some(open(&self.path)?);
        self.instance += 1;
        bump(&mut self.metrics, "actual_close_reopens");
        self.verify()
    }
    fn observe(&mut self, slot: u8) -> Result<(), String> {
        let observation = self.journal()?.observe_replay().map_err(error)?;
        self.markers[usize::from(slot) % 4] = Some(Marker { observation, version: self.version() });
        bump(&mut self.metrics, "replay_observations");
        Ok(())
    }
    fn observed_append(&mut self, slot: u8) -> Result<(), String> {
        let index = usize::from(slot) % 4;
        if self.markers[index].is_none() {
            self.observe(slot)?;
        }
        let marker = self.markers[index].take().ok_or("marker missing")?;
        let expected = self.current(marker.version);
        let packet = self.packet(slot % 4, slot, false, 2)?;
        self.attempt += 1;
        let result = self.journal()?.append_observed(packet.plan()?, &marker.observation);
        match (expected, result) {
            (true, Ok((batch, observation))) => {
                self.commit_model(packet, &batch)?;
                self.markers[index] = Some(Marker { observation, version: self.version() });
                bump(&mut self.metrics, "guarded_commits");
            }
            (false, Err(e)) if e.kind() == JournalErrorKind::StaleHead => {
                self.markers[index] = Some(marker);
                bump(&mut self.metrics, "stale_observation_rejections");
            }
            (expected, result) => {
                return Err(format!("guarded append current={expected}, actual={result:?}"));
            }
        }
        self.verify()
    }
    fn retry(&mut self, selector: u8, conflict: bool) -> Result<(), String> {
        if self.commands.is_empty() {
            self.fresh(0, 7, false, false, 2)?;
        }
        let packet = self.commands[usize::from(selector) % self.commands.len()].clone();
        self.attempt += 1;
        let result =
            self.journal()?.append(packet.request(false, conflict)?.plan().map_err(error)?);
        if conflict {
            require_error(result, JournalErrorKind::IdempotencyConflict)?;
            bump(&mut self.metrics, "conflicting_retries");
        } else {
            verify_batch(&result.map_err(error)?, &packet)?;
            bump(&mut self.metrics, "exact_retries");
        }
        self.verify()
    }
    fn reject(&mut self, aggregate: u8, kind: Rejection) -> Result<(), String> {
        if !self.events.iter().any(|e| e.aggregate == aggregate) {
            self.fresh(aggregate, 19, false, false, 2)?;
        }
        let mut packet = self.packet(aggregate, 88, true, 2)?;
        if matches!(kind, Rejection::Head) {
            packet.head = None;
            packet.event.sequence = 1;
            packet.event.previous = None;
            packet.event.previous_hash = Sha256Digest::new([0; 32]);
        }
        if matches!(kind, Rejection::State) {
            packet.state = None;
        }
        let external = Connection::open(&self.path).map_err(error)?;
        match kind {
            Rejection::Locked => external.execute_batch("BEGIN IMMEDIATE").map_err(error)?,
            Rejection::Transaction => {
                external.execute_batch("CREATE TRIGGER praxis_abort BEFORE INSERT ON commands BEGIN SELECT RAISE(ABORT,'owned injected command-row failure'); END").map_err(error)?;
                self.external += 1;
            }
            _ => {}
        }
        self.attempt += 1;
        let result = self.journal()?.append(
            packet.request(matches!(kind, Rejection::WrongStore), false)?.plan().map_err(error)?,
        );
        let expected = match kind {
            Rejection::Locked => JournalErrorKind::Busy,
            Rejection::Transaction => JournalErrorKind::Storage,
            Rejection::WrongStore => JournalErrorKind::InvalidInput,
            _ => JournalErrorKind::StaleHead,
        };
        let checked = match result {
            Err(failure)
                if matches!(kind, Rejection::Transaction)
                    && (failure.operation() != "record command result"
                        || !std::error::Error::source(&failure).is_some_and(|source| {
                            source.to_string().contains("owned injected command-row failure")
                        })) =>
            {
                Err(format!(
                    "transaction fault did not reach the declared command-row boundary: {failure:?}"
                ))
            }
            other => require_error(other, expected),
        };
        match kind {
            Rejection::Locked => external.execute_batch("ROLLBACK").map_err(error)?,
            Rejection::Transaction => {
                external.execute_batch("DROP TRIGGER praxis_abort").map_err(error)?;
                self.external += 1;
            }
            _ => {}
        }
        checked?;
        bump(&mut self.metrics, &format!("rejected_{kind:?}"));
        let resolution = self
            .journal()?
            .resolve_command(
                CommandId::new(bytes(packet.event.number)).map_err(error)?,
                packet.digest,
            )
            .map_err(error)?;
        if !matches!(resolution, CommandResolution::DefinitelyAbsent) {
            return Err("failed transaction retained a command receipt".into());
        }
        if self
            .journal()?
            .outbox_message(OutboxId::new(bytes(packet.event.number)).map_err(error)?)
            .map_err(error)?
            .is_some()
        {
            return Err("failed transaction leaked outbox row".into());
        }
        self.verify()?;
        bump(&mut self.metrics, "continued_after_rejection");
        Ok(())
    }
    fn external_commit(&mut self) -> Result<(), String> {
        let connection = Connection::open(&self.path).map_err(error)?;
        connection
            .execute_batch("CREATE TABLE IF NOT EXISTS praxis_probe(value INTEGER)")
            .map_err(error)?;
        connection
            .execute(
                "INSERT INTO praxis_probe(value) VALUES(?1)",
                [i64::try_from(self.next).map_err(error)?],
            )
            .map_err(error)?;
        self.external += 1;
        bump(&mut self.metrics, "unrelated_external_sqlite_commits");
        self.verify()
    }
    fn invalid_plan(&mut self) -> Result<(), String> {
        let request = AppendRequest::new(
            store_id()?,
            CommandId::new(bytes(99999)).map_err(error)?,
            hash(b"empty plan"),
            vec![HeadExpectation::Absent(key(0)?)],
            vec![],
            vec![],
            vec![],
            None,
            None,
            vec![],
        );
        require_error(request.plan(), JournalErrorKind::EmptyBatch)?;
        bump(&mut self.metrics, "rejected_before_append");
        self.verify()
    }
    fn claim(&mut self, advance: u8, duration: u8) -> Result<(), String> {
        self.now += u64::from(advance);
        let now = self.now;
        let until = now + u64::from(duration);
        for row in self.deliveries.values_mut() {
            if row.attempts >= row.maximum
                && (row.state == OutboxState::Pending
                    || (row.state == OutboxState::Claimed
                        && row.until.is_some_and(|lease| lease <= now)))
            {
                row.state = OutboxState::Exhausted;
                row.until = None;
            }
        }
        let selected = self
            .deliveries
            .values()
            .filter(|row| {
                row.state == OutboxState::Pending
                    || (row.state == OutboxState::Claimed
                        && row.until.is_some_and(|lease| lease <= now))
            })
            .filter(|row| row.attempts < row.maximum)
            .min_by_key(|row| (row.position, row.number))
            .map(|row| row.number);
        if let Some(number) = selected {
            let row = self.deliveries.get_mut(&number).ok_or("selected delivery absent")?;
            row.attempts += 1;
            row.state = OutboxState::Claimed;
            row.fence = Some(row.fence.unwrap_or(0) + 1);
            row.until = Some(until);
        }
        let actual = self.journal()?.claim_outbox(now, until).map_err(error)?;
        if actual.as_ref().map(|row| *row.id().as_bytes()) != selected.map(bytes) {
            return Err("outbox claim selected wrong eligible message".into());
        }
        if actual.is_some() {
            bump(&mut self.metrics, "delivery_claims");
        } else {
            bump(&mut self.metrics, "empty_claims");
        }
        self.verify()
    }
    fn acknowledge(&mut self, selector: u8, fence: Fence) -> Result<(), String> {
        if self.deliveries.is_empty() {
            self.fresh(0, 31, true, false, 2)?;
        }
        let number = *self
            .deliveries
            .keys()
            .nth(usize::from(selector) % self.deliveries.len())
            .ok_or("delivery absent")?;
        let row = self.deliveries.get(&number).ok_or("delivery absent")?;
        let current = row.fence.unwrap_or(1);
        let supplied = match fence {
            Fence::Current => current,
            Fence::Old => current.saturating_sub(1).max(1),
            Fence::Future => current + 1,
            Fence::Zero => 0,
        };
        let expected = if supplied == 0 {
            Some(JournalErrorKind::InvalidInput)
        } else if row.state == OutboxState::Acknowledged
            || (row.state == OutboxState::Claimed && row.fence == Some(supplied))
        {
            None
        } else {
            Some(JournalErrorKind::StaleHead)
        };
        let actual = self
            .journal()?
            .acknowledge_outbox(OutboxId::new(bytes(number)).map_err(error)?, supplied);
        match expected {
            Some(kind) => {
                require_error(actual, kind)?;
                bump(&mut self.metrics, "stale_delivery_acknowledgements");
            }
            None => {
                actual.map_err(error)?;
                let row = self.deliveries.get_mut(&number).ok_or("delivery absent")?;
                row.state = OutboxState::Acknowledged;
                row.until = None;
                bump(&mut self.metrics, "delivery_acknowledgements");
            }
        }
        self.verify()
    }
    pub fn verify(&mut self) -> Result<(), String> {
        let export = self.journal()?.integrity_export().map_err(error)?;
        if export.records().len() != self.events.len()
            || export.report().last_position() != u64::try_from(self.events.len()).map_err(error)?
            || export.report().journal_head_digest() != expected_head(&self.events)?
        {
            return Err("global append list differs from committed export".into());
        }
        for (actual, expected) in export.records().iter().zip(&self.events) {
            verify_event(actual, expected)?;
        }
        for aggregate in 0..4 {
            let rows = self.journal()?.records_for_aggregate(key(aggregate)?).map_err(error)?;
            let expected = self
                .events
                .iter()
                .filter(|e| e.aggregate == aggregate)
                .cloned()
                .collect::<Vec<_>>();
            if rows.len() != expected.len() {
                return Err("aggregate replay lost or duplicated events".into());
            }
            for (actual, expected) in rows.iter().zip(&expected) {
                verify_event(actual, expected)?;
            }
            verify_head(self.journal()?.head(key(aggregate)?).map_err(error)?, expected.last())?;
            let current =
                self.journal()?.state_record(STATE_NAMESPACE, &[aggregate]).map_err(error)?;
            if let Some(expected) = expected.last() {
                verify_state(current.as_ref().ok_or("state missing")?, expected)?;
            } else if current.is_some() {
                return Err("state exists without a committed event".into());
            }
            for expected in &expected {
                let historical = self
                    .journal()?
                    .state_record_revision(STATE_NAMESPACE, &[aggregate], expected.sequence)
                    .map_err(error)?
                    .ok_or("immutable state history missing")?;
                verify_state(&historical, expected)?;
            }
            let next_revision = expected.last().map_or(1, |event| event.sequence + 1);
            if self
                .journal()?
                .state_record_revision(STATE_NAMESPACE, &[aggregate], next_revision)
                .map_err(error)?
                .is_some()
            {
                return Err("uncommitted next state revision leaked into immutable history".into());
            }
        }
        for marker in self.markers.iter().flatten() {
            let actual = self
                .journal
                .as_ref()
                .ok_or("journal closed")?
                .replay_observation_is_current(&marker.observation)
                .map_err(error)?;
            if actual != self.current(marker.version) {
                return Err(
                    "replay observation freshness differs from instance/attempt/external model"
                        .into(),
                );
            }
        }
        for row in self.deliveries.values() {
            let actual = self
                .journal
                .as_ref()
                .ok_or("journal closed")?
                .outbox_message(OutboxId::new(bytes(row.number)).map_err(error)?)
                .map_err(error)?
                .ok_or("delivery row missing")?;
            if actual.state() != row.state
                || actual.attempts() != row.attempts
                || actual.fence() != row.fence
                || actual.lease_until() != row.until
                || actual.producing_position() != row.position
                || actual.payload() != [row.value]
                || actual.max_attempts() != row.maximum
                || actual.destination() != "fixture://delivery"
            {
                return Err(format!(
                    "delivery differs from lease model: {actual:?}; expected {row:?}"
                ));
            }
        }
        bump(&mut self.metrics, "independent_state_observations");
        Ok(())
    }
}

fn open(path: &Path) -> Result<SqliteJournal, String> {
    SqliteJournal::open(path, store_id()?, SqliteJournalOptions { busy_timeout: Duration::ZERO })
        .map_err(error)
}
fn require_error<T: std::fmt::Debug>(
    actual: Result<T, JournalError>,
    expected: JournalErrorKind,
) -> Result<(), String> {
    match actual {
        Err(e) if e.kind() == expected => Ok(()),
        other => Err(format!("expected {expected:?}, observed {other:?}")),
    }
}
fn verify_head(actual: Option<AggregateHead>, expected: Option<&Event>) -> Result<(), String> {
    match (actual, expected) {
        (None, None) => Ok(()),
        (Some(actual), Some(expected))
            if actual.key() == key(expected.aggregate)?
                && actual.sequence().get() == expected.sequence
                && actual.event_id().as_bytes() == &bytes(expected.number)
                && actual.event_hash() == expected.event_hash =>
        {
            Ok(())
        }
        other => Err(format!("aggregate head mismatch: {other:?}")),
    }
}
fn verify_event(actual: &CommittedRecord, expected: &Event) -> Result<(), String> {
    if actual.global_position() != expected.position
        || actual.aggregate() != key(expected.aggregate)?
        || actual.sequence().get() != expected.sequence
        || actual.event_id().as_bytes() != &bytes(expected.number)
        || actual.previous_event_id().map(|id| *id.as_bytes()) != expected.previous.map(bytes)
        || actual.previous_event_hash() != expected.previous_hash
        || actual.event_hash() != expected.event_hash
        || actual.command_id().as_bytes() != &bytes(expected.number)
        || actual.frame_bytes() != expected.frame
        || actual.frame_family() != expected.family
        || actual.frame_digest() != hash(&expected.frame)
        || actual.revision_digest() != revision(expected.aggregate)
        || !actual.causal_parents().is_empty()
    {
        return Err(format!("exact immutable event mismatch: {actual:?}; expected {expected:?}"));
    }
    Ok(())
}
fn verify_state(
    actual: &peritus_journal::DurableStateRecord,
    expected: &Event,
) -> Result<(), String> {
    if actual.namespace() != STATE_NAMESPACE
        || actual.key() != [expected.aggregate]
        || actual.revision() != expected.sequence
        || actual.bytes() != [expected.value]
        || actual.digest() != hash(&[expected.value])
        || actual.producing_position() != expected.position
    {
        return Err("current/historical state differs from exact accepted bytes".into());
    }
    Ok(())
}
fn verify_batch(batch: &CommittedBatch, packet: &Packet) -> Result<(), String> {
    if batch.command_id().as_bytes() != &bytes(packet.event.number)
        || batch.request_digest() != packet.digest
        || batch.first_position() != packet.event.position
        || batch.last_position() != packet.event.position
        || batch.records().len() != 1
        || batch.heads().len() != 1
        || !batch.artifact_dependencies().is_empty()
        || batch.batch_hash() != batch_hash(packet)
    {
        return Err("command replay receipt differs from original committed request".into());
    }
    verify_event(&batch.records()[0], &packet.event)?;
    verify_head(Some(batch.heads()[0]), Some(&packet.event))
}

fn batch_key(index: u8) -> Result<AggregateKey, String> {
    Ok(AggregateKey::new(
        AggregateKind::Trace,
        AggregateId::new(bytes(200 + u64::from(index))).map_err(error)?,
    ))
}
fn batch_request(conflict: bool) -> Result<AppendRequest, String> {
    let mut heads = Vec::new();
    let mut events = Vec::new();
    let mut states = Vec::new();
    let mut outbox = Vec::new();
    for index in 0_u8..2 {
        let aggregate = batch_key(index)?;
        heads.push(HeadExpectation::Absent(aggregate));
        events.push(
            EventDraft::new(
                aggregate,
                EventSequence::new(1).map_err(error)?,
                EventId::new(bytes(2001 + u64::from(index))).map_err(error)?,
                None,
                ExactFrame::new(frame(1 + u16::from(index), 9 + index)).map_err(error)?,
                Sha256Digest::new([77; 32]),
                vec![],
            )
            .map_err(error)?,
        );
        states.push(
            StateInstall::new(702, vec![index + 1], None, 1, vec![9 + index]).map_err(error)?,
        );
        outbox.push(
            OutboxDraft::new(
                OutboxId::new(bytes(3001 + u64::from(index))).map_err(error)?,
                "fixture://atomic".into(),
                vec![9 + index],
                2,
            )
            .map_err(error)?,
        );
    }
    Ok(AppendRequest::new(
        store_id()?,
        CommandId::new(bytes(2000)).map_err(error)?,
        hash(if conflict { b"conflicting atomic request" } else { b"exact atomic request" }),
        heads,
        events,
        states,
        vec![],
        None,
        None,
        outbox,
    ))
}
fn batch_event_hash(index: u8) -> Sha256Digest {
    hash(
        &[
            b"peritus.journal.event.v1\0".as_slice(),
            &8_u16.to_be_bytes(),
            &bytes(200 + u64::from(index)),
            &1_u64.to_be_bytes(),
            &bytes(2001 + u64::from(index)),
            &[0],
            &[0; 16],
            &[0; 32],
            &bytes(2000),
            hash(&frame(1 + u16::from(index), 9 + index)).as_bytes(),
            &[77; 32],
            &0_u32.to_be_bytes(),
        ]
        .concat(),
    )
}
fn verify_atomic(journal: &mut SqliteJournal, present: bool) -> Result<(), String> {
    let export = journal.integrity_export().map_err(error)?;
    if export.records().len() != if present { 2 } else { 0 } {
        return Err("atomic batch retained only part of its event set".into());
    }
    for index in 0_u8..2 {
        let head = journal.head(batch_key(index)?).map_err(error)?;
        let state = journal.state_record(702, &[index + 1]).map_err(error)?;
        let history = journal.state_record_revision(702, &[index + 1], 1).map_err(error)?;
        let outbox = journal
            .outbox_message(OutboxId::new(bytes(3001 + u64::from(index))).map_err(error)?)
            .map_err(error)?;
        if !present {
            if head.is_some() || state.is_some() || history.is_some() || outbox.is_some() {
                return Err("failed second insert leaked head/state/history/outbox".into());
            }
            continue;
        }
        let event = &export.records()[usize::from(index)];
        if event.global_position() != u64::from(index) + 1
            || event.aggregate() != batch_key(index)?
            || event.sequence().get() != 1
            || event.command_id().as_bytes() != &bytes(2000)
            || event.event_id().as_bytes() != &bytes(2001 + u64::from(index))
            || event.event_hash() != batch_event_hash(index)
            || event.frame_bytes() != frame(1 + u16::from(index), 9 + index)
        {
            return Err("multi-aggregate batch event differs from independent input tuple".into());
        }
        let head = head.ok_or("batch head missing")?;
        if head.sequence().get() != 1
            || head.event_id() != event.event_id()
            || head.event_hash() != batch_event_hash(index)
        {
            return Err("batch head not atomically advanced".into());
        }
        for row in [state.ok_or("batch state missing")?, history.ok_or("batch history missing")?] {
            if row.bytes() != [9 + index]
                || row.revision() != 1
                || row.producing_position() != 2
                || row.digest() != hash(&[9 + index])
            {
                return Err(
                    "multi-aggregate state install differs from exact batch boundary".into()
                );
            }
        }
        let outbox = outbox.ok_or("batch outbox missing")?;
        if outbox.payload() != [9 + index]
            || outbox.producing_position() != 2
            || outbox.state() != OutboxState::Pending
            || outbox.attempts() != 0
            || outbox.destination() != "fixture://atomic"
        {
            return Err("batch outbox publication changed".into());
        }
    }
    Ok(())
}
fn verify_atomic_receipt(committed: &CommittedBatch) -> Result<(), String> {
    let batch_digest = hash(
        &[
            b"peritus.journal.batch.v1\0".as_slice(),
            &[41; 16],
            &bytes(2000),
            hash(b"exact atomic request").as_bytes(),
            &2_u32.to_be_bytes(),
            batch_event_hash(0).as_bytes(),
            batch_event_hash(1).as_bytes(),
            &0_u32.to_be_bytes(),
        ]
        .concat(),
    );
    if committed.command_id().as_bytes() != &bytes(2000)
        || committed.request_digest() != hash(b"exact atomic request")
        || committed.first_position() != 1
        || committed.last_position() != 2
        || committed.batch_hash() != batch_digest
    {
        return Err("atomic batch range/hash differs from independent preimage".into());
    }
    if committed.records().len() != 2
        || committed.heads().len() != 2
        || !committed.artifact_dependencies().is_empty()
    {
        return Err("atomic receipt omitted/added records or affected heads".into());
    }
    for index in 0_u8..2 {
        let head = committed.heads()[usize::from(index)];
        let event = &committed.records()[usize::from(index)];
        if head.key() != batch_key(index)?
            || head.sequence().get() != 1
            || head.event_id().as_bytes() != &bytes(2001 + u64::from(index))
            || head.event_hash() != batch_event_hash(index)
            || event.global_position() != u64::from(index) + 1
            || event.aggregate() != batch_key(index)?
            || event.sequence().get() != 1
            || event.command_id().as_bytes() != &bytes(2000)
            || event.event_id().as_bytes() != &bytes(2001 + u64::from(index))
            || event.frame_bytes() != frame(1 + u16::from(index), 9 + index)
            || event.event_hash() != batch_event_hash(index)
            || event.revision_digest() != Sha256Digest::new([77; 32])
            || event.previous_event_id().is_some()
            || event.previous_event_hash() != Sha256Digest::new([0; 32])
            || !event.causal_parents().is_empty()
        {
            return Err("atomic receipt record/head differs from independent input tuple".into());
        }
    }
    Ok(())
}

fn atomic_batch_wave() -> Result<Metrics, String> {
    let directory = tempfile::tempdir().map_err(error)?;
    let path = directory.path().join("batch.sqlite");
    let mut journal = open(&path)?;
    let connection = Connection::open(&path).map_err(error)?;
    connection.execute_batch("CREATE TRIGGER praxis_second_insert BEFORE INSERT ON events WHEN NEW.frame_family=2 AND (SELECT COUNT(*) FROM events)=1 BEGIN SELECT RAISE(ABORT,'owned second-event failure'); END").map_err(error)?;
    let failure = journal.append(batch_request(false)?.plan().map_err(error)?);
    match failure {
        Err(failure)
            if failure.kind() == JournalErrorKind::Storage
                && failure.operation() == "insert event"
                && std::error::Error::source(&failure).is_some_and(|source| {
                    source.to_string().contains("owned second-event failure")
                }) => {}
        other => {
            return Err(format!(
                "fault did not activate after the first inserted event: {other:?}"
            ));
        }
    }
    verify_atomic(&mut journal, false)?;
    if !matches!(
        journal
            .resolve_command(
                CommandId::new(bytes(2000)).map_err(error)?,
                hash(b"exact atomic request")
            )
            .map_err(error)?,
        CommandResolution::DefinitelyAbsent
    ) {
        return Err("rolled-back batch retained a command result".into());
    }
    connection.execute_batch("DROP TRIGGER praxis_second_insert").map_err(error)?;
    let committed = journal.append(batch_request(false)?.plan().map_err(error)?).map_err(error)?;
    verify_atomic_receipt(&committed)?;
    verify_atomic(&mut journal, true)?;
    let replay = journal.append(batch_request(false)?.plan().map_err(error)?).map_err(error)?;
    if replay != committed {
        return Err("exact multi-aggregate retry changed original receipt".into());
    }
    require_error(
        journal.append(batch_request(true)?.plan().map_err(error)?),
        JournalErrorKind::IdempotencyConflict,
    )?;
    drop(connection);
    drop(journal);
    let mut journal = open(&path)?;
    verify_atomic(&mut journal, true)?;
    Ok(Metrics::from([
        ("multi_aggregate_batches".into(), 1),
        ("batch_second_event_failures".into(), 1),
        ("batch_exact_retries".into(), 1),
        ("batch_conflicting_retries".into(), 1),
        ("batch_atomic_reopens".into(), 1),
    ]))
}

fn lease_scope() -> Result<peritus_leases::LeaseScope, String> {
    Ok(peritus_leases::LeaseScope::new(
        peritus_types::WorkspaceId::new([71; 16]).map_err(error)?,
        peritus_types::ResourceId::new([72; 16]).map_err(error)?,
        peritus_types::EnvironmentId::new([73; 16]).map_err(error)?,
    ))
}
fn lease_mint(number: u64) -> Result<peritus_leases::LeaseTransition, String> {
    peritus_leases::LeaseAggregate::mint(peritus_leases::MintLease::new(
        CommandId::new(bytes(number)).map_err(error)?,
        lease_scope()?,
        peritus_policy::AuthorityInstant::new(peritus_types::Generation::first(), 10),
    ))
    .map_err(error)
}
fn lease_acquire(
    aggregate: peritus_leases::LeaseAggregate,
) -> Result<peritus_leases::LeaseTransition, String> {
    match aggregate.acquire(peritus_leases::AcquireLease::new(
        CommandId::new(bytes(1001)).map_err(error)?,
        peritus_leases::LeaseHolder::new(
            peritus_types::ActorId::new([74; 16]).map_err(error)?,
            peritus_types::SessionId::new([75; 16]).map_err(error)?,
        ),
        peritus_leases::LeaseDuration::new(100).map_err(error)?,
        peritus_policy::AuthorityInstant::new(peritus_types::Generation::first(), 10),
    )) {
        peritus_leases::LeaseTransitionOutcome::Accepted(value) => Ok(value),
        peritus_leases::LeaseTransitionOutcome::Rejected(failure) => {
            Err(format!("known valid acquisition rejected: {:?}", failure.error()))
        }
    }
}
fn lease_key() -> Result<AggregateKey, String> {
    Ok(AggregateKey::new(AggregateKind::Lease, AggregateId::new(bytes(999)).map_err(error)?))
}
fn lease_append(
    head: Option<AggregateHead>,
    version: u64,
    number: u64,
) -> Result<AppendRequest, String> {
    let aggregate = lease_key()?;
    let event = EventDraft::new(
        aggregate,
        EventSequence::new(version).map_err(error)?,
        EventId::new(bytes(number)).map_err(error)?,
        if version == 1 { None } else { Some(EventId::new(bytes(number - 1)).map_err(error)?) },
        ExactFrame::new(frame(300, u8::try_from(version).map_err(error)?)).map_err(error)?,
        Sha256Digest::new([76; 32]),
        vec![],
    )
    .map_err(error)?;
    Ok(AppendRequest::new(
        store_id()?,
        CommandId::new(bytes(number)).map_err(error)?,
        hash(&bytes(number)),
        vec![head.map_or(HeadExpectation::Absent(aggregate), HeadExpectation::Present)],
        vec![event],
        vec![],
        vec![],
        None,
        None,
        vec![],
    ))
}
fn lease_state_bytes(version: u64, number: u64) -> Result<Vec<u8>, String> {
    // Fixed logical mint/acquire truth, encoded independently of the C0 transition serializer.
    let scope = [[71; 16], [72; 16], [73; 16]].concat();
    let mut payload = bytes(number).to_vec();
    payload.extend(&scope);
    if version == 1 {
        payload.push(0);
    } else {
        payload.push(1);
        payload.extend(1_u64.to_be_bytes());
    }
    payload.extend(version.to_be_bytes());
    if version == 1 {
        payload.push(0);
    } else {
        payload.push(1);
        payload.extend(1_u64.to_be_bytes());
    }
    payload.extend(1_u64.to_be_bytes());
    if version == 1 {
        payload.push(0);
    } else {
        payload.extend([1, 1]);
    }
    let phase = if version == 1 { 1 } else { 2 };
    payload.extend([phase, phase]);
    for value in [1, version, 1, 10] {
        payload.extend(value.to_be_bytes());
    }
    payload.extend([phase, u8::from(version == 2)]);
    if version == 2 {
        payload.extend(&scope);
        payload.extend([74; 16]);
        payload.extend([75; 16]);
        for value in [1_u64, 1, 1, 10, 1, 110] {
            payload.extend(value.to_be_bytes());
        }
    }
    payload.extend([0, 0, 0]);
    let mut result = b"PERITUS-C0-STATE\0".to_vec();
    result.extend(1_u16.to_be_bytes());
    result.extend(3_u16.to_be_bytes());
    result.extend(u64::try_from(payload.len()).map_err(error)?.to_be_bytes());
    result.extend(payload);
    Ok(result)
}
fn lease_request_digest(version: u64, number: u64, state: &[u8]) -> Sha256Digest {
    let domain = b"peritus.lease-transition.v1";
    let mut input = b"PERITUS-C0-DOMAIN-STATE\0".to_vec();
    input.extend((domain.len() as u64).to_be_bytes());
    input.extend(domain);
    input.extend(hash(&bytes(number)).as_bytes());
    input.extend(1_u64.to_be_bytes());
    input.extend(103_u16.to_be_bytes());
    input.extend(48_u64.to_be_bytes());
    input.extend([[71; 16], [72; 16], [73; 16]].concat());
    if version == 1 {
        input.push(0);
    } else {
        input.push(1);
        input.extend((version - 1).to_be_bytes());
    }
    input.extend(version.to_be_bytes());
    input.extend(hash(state).as_bytes());
    hash(&input)
}
fn typed_lease_wave() -> Result<Metrics, String> {
    let directory = tempfile::tempdir().map_err(error)?;
    let path = directory.path().join("lease.sqlite");
    let mut journal = open(&path)?;
    let state_key = [[71; 16], [72; 16], [73; 16]].concat();
    let mut aggregate = None;
    let mut acquisition_head = None;
    for version in 1_u64..=2 {
        let number = 999 + version;
        let expected = lease_state_bytes(version, number)?;
        let head = journal.head(lease_key()?).map_err(error)?;
        if head.map(|h| h.sequence().get()) != if version == 1 { None } else { Some(1) } {
            return Err("typed lease head prerequisite differs".into());
        }
        if version == 2 {
            acquisition_head = head;
        }
        let transition = if version == 1 {
            lease_mint(number)?
        } else {
            lease_acquire(aggregate.take().ok_or("mint successor missing")?)?
        };
        let committed = journal
            .commit_lease_transition(
                peritus_journal::LeaseCommitRequest::new(
                    lease_append(head, version, number)?,
                    transition,
                )
                .map_err(error)?,
            )
            .map_err(error)?;
        if committed.state_revision() != version
            || committed.state_digest() != hash(&expected)
            || committed.batch().last_position() != version
            || committed.batch().request_digest()
                != lease_request_digest(version, number, &expected)
        {
            return Err(
                "typed lease receipt differs from independently encoded mint/acquire truth".into(),
            );
        }
        let state =
            journal.state_record(103, &state_key).map_err(error)?.ok_or("typed state absent")?;
        if state.bytes() != expected
            || state.revision() != version
            || state.producing_position() != version
        {
            return Err("typed lease exact state differs".into());
        }
        aggregate = Some(committed.into_parts().1);
    }
    let reconstructed = lease_acquire(lease_mint(1000)?.into_next())?;
    let replay = journal
        .commit_lease_transition(
            peritus_journal::LeaseCommitRequest::new(
                lease_append(acquisition_head, 2, 1001)?,
                reconstructed,
            )
            .map_err(error)?,
        )
        .map_err(error)?;
    if replay.batch().first_position() != 2
        || replay.batch().last_position() != 2
        || replay.state_revision() != 2
    {
        return Err("typed lease retry appended or changed original receipt".into());
    }
    let observed = journal.observe_replay().map_err(error)?;
    let stale = journal.commit_lease_transition(
        peritus_journal::LeaseCommitRequest::new(
            lease_append(journal.head(lease_key()?).map_err(error)?, 3, 1002)?,
            lease_mint(1002)?,
        )
        .map_err(error)?,
    );
    require_error(stale.map(|_| ()), JournalErrorKind::StaleHead)?;
    if journal.replay_observation_is_current(&observed).map_err(error)? {
        return Err("typed failed CAS retained replay freshness".into());
    }
    drop(journal);
    let mut journal = open(&path)?;
    for version in 1..=2 {
        let state = journal
            .state_record_revision(103, &state_key, version)
            .map_err(error)?
            .ok_or("typed history lost after reopen")?;
        if state.bytes() != lease_state_bytes(version, 999 + version)?
            || state.producing_position() != version
        {
            return Err("reopened typed lease history changed".into());
        }
    }
    if journal.integrity_scan().map_err(error)?.event_count() != 2 {
        return Err("typed failed CAS/retry changed committed event count".into());
    }
    Ok(Metrics::from([
        ("typed_lease_commits".into(), 2),
        ("typed_lease_replays".into(), 1),
        ("typed_lease_state_cas_rejections".into(), 1),
        ("typed_lease_reopened_histories".into(), 2),
    ]))
}

pub fn check(case: &Case) -> Result<Metrics, String> {
    if case.steps.len() > 96 {
        return Err("setup-incomplete: journal history exceeds bounded campaign".into());
    }
    let mut fixture = Fixture::new()?;
    for (index, step) in case.steps.iter().enumerate() {
        let result = match *step {
            Op::Append { aggregate, value, outbox, external, maximum } => {
                fixture.fresh(aggregate % 4, value, outbox, external, maximum)
            }
            Op::Retry { selector, conflict } => fixture.retry(selector, conflict),
            Op::Observe(slot) => fixture.observe(slot),
            Op::ObservedAppend(slot) => fixture.observed_append(slot),
            Op::Reject { aggregate, kind } => fixture.reject(aggregate % 4, kind),
            Op::InvalidPlan => fixture.invalid_plan(),
            Op::ExternalCommit => fixture.external_commit(),
            Op::Reopen => fixture.reopen(),
            Op::Claim { advance, duration } => fixture.claim(advance, duration),
            Op::Acknowledge { selector, fence } => fixture.acknowledge(selector, fence),
            Op::TypedOwners => {
                for (key, count) in
                    typed_lease_wave()?.into_iter().chain(super::approval::durable_approval_wave()?)
                {
                    *fixture.metrics.entry(key).or_default() += count;
                }
                Ok(())
            }
            Op::AtomicBatch => {
                for (key, count) in atomic_batch_wave()? {
                    *fixture.metrics.entry(key).or_default() += count;
                }
                Ok(())
            }
        };
        result.map_err(|e| format!("{e}; step {index} {step:?}"))?;
        fixture.verify()?;
        bump(&mut fixture.metrics, "operations");
    }
    fixture.verify()?;
    Ok(fixture.metrics)
}
fn append(aggregate: u8, value: u8, outbox: bool, maximum: u8) -> Op {
    Op::Append { aggregate, value, outbox, external: false, maximum }
}
pub fn calibration() -> Case {
    Case { steps: vec![Op::Observe(0), append(0, 1, false, 2), Op::ObservedAppend(0)] }
}
pub fn outbox_lease_case() -> Case {
    Case {
        steps: vec![
            append(0, 7, true, 1),
            Op::Claim { advance: 0, duration: 10 },
            Op::Claim { advance: 1, duration: 2 },
            Op::Acknowledge { selector: 0, fence: Fence::Current },
            Op::Reopen,
        ],
    }
}
pub fn examples() -> Vec<Case> {
    vec![
        calibration(),
        Case { steps: vec![Op::TypedOwners, Op::Reopen] },
        Case { steps: vec![Op::AtomicBatch, Op::Reopen] },
        outbox_lease_case(),
        Case {
            steps: vec![
                append(0, 8, true, 2),
                append(1, 9, false, 2),
                Op::Observe(0),
                Op::Retry { selector: 0, conflict: false },
                Op::ObservedAppend(0),
                Op::Observe(1),
                Op::Reject { aggregate: 0, kind: Rejection::Head },
                Op::ObservedAppend(1),
                Op::Retry { selector: 0, conflict: true },
                Op::Reject { aggregate: 1, kind: Rejection::State },
                Op::Reject { aggregate: 0, kind: Rejection::Locked },
                Op::Reject { aggregate: 0, kind: Rejection::Transaction },
                Op::InvalidPlan,
                Op::Observe(2),
                Op::ExternalCommit,
                Op::ObservedAppend(2),
                Op::Reopen,
            ],
        },
        Case {
            steps: vec![
                append(0, 1, true, 2),
                Op::Claim { advance: 0, duration: 1 },
                Op::Claim { advance: 2, duration: 1 },
                Op::Acknowledge { selector: 0, fence: Fence::Old },
                Op::Claim { advance: 2, duration: 1 },
                Op::Reopen,
            ],
        },
    ]
}

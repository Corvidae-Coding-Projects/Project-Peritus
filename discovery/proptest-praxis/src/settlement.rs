//! Candidate qualification and terminal truth against an independent explicit state model.
use peritus_run_settlement::{
    CandidateCheckpoint, CandidateIdentity, CandidateStage, EvidenceDependencies, EvidenceRecord,
    EvidenceStatus, QualificationEvidence, RunDisposition, RunSettlement, SettlementCause,
    SettlementError, SettlementErrorKind, SettlementReducer,
};
use peritus_types::{RunId, Sha256Digest, WorkspaceId};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub operations: Vec<Operation>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Axis {
    Run,
    Workspace,
    Content,
    Repository,
    Execution,
    Conversation,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Sequence {
    Next,
    Same,
    Backwards,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Cause {
    Completed,
    NoChange,
    Wait,
    Cancel,
    Deadline,
    Provider,
    Context,
    Gate,
    Review,
    Repository,
    Adapter,
    Recovery,
    Invariant,
}
impl Cause {
    fn actual(self) -> SettlementCause {
        match self {
            Self::Completed => SettlementCause::Completed,
            Self::NoChange => SettlementCause::NoChangeComplete,
            Self::Wait => SettlementCause::UserWait,
            Self::Cancel => SettlementCause::Cancellation,
            Self::Deadline => SettlementCause::Deadline,
            Self::Provider => SettlementCause::Provider,
            Self::Context => SettlementCause::Context,
            Self::Gate => SettlementCause::Gate,
            Self::Review => SettlementCause::Review,
            Self::Repository => SettlementCause::Repository,
            Self::Adapter => SettlementCause::Adapter,
            Self::Recovery => SettlementCause::Recovery,
            Self::Invariant => SettlementCause::InternalInvariant,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    Record {
        stage: u8,
        acquire: u8,
        strict: bool,
        sequence: Sequence,
    },
    Change {
        axis: Axis,
        value: u8,
    },
    Reobserve {
        sequence: Sequence,
    },
    ProbeEvidence {
        slot: u8,
        status: u8,
        satisfied: bool,
        drift: Option<Axis>,
        future: bool,
        strict: bool,
        stage: u8,
    },
    Settle {
        cause: Cause,
    },
    Restart {
        retain: bool,
        clear_execution: bool,
    },
    Read,
}
fn axis_strategy() -> BoxedStrategy<Axis> {
    prop_oneof![
        Just(Axis::Run),
        Just(Axis::Workspace),
        Just(Axis::Content),
        Just(Axis::Repository),
        Just(Axis::Execution),
        Just(Axis::Conversation)
    ]
    .boxed()
}
fn sequence_strategy() -> BoxedStrategy<Sequence> {
    prop_oneof![5=>Just(Sequence::Next),1=>Just(Sequence::Same),1=>Just(Sequence::Backwards)]
        .boxed()
}
fn causes() -> Vec<Cause> {
    vec![
        Cause::Completed,
        Cause::NoChange,
        Cause::Wait,
        Cause::Cancel,
        Cause::Deadline,
        Cause::Provider,
        Cause::Context,
        Cause::Gate,
        Cause::Review,
        Cause::Repository,
        Cause::Adapter,
        Cause::Recovery,
        Cause::Invariant,
    ]
}
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let operation = prop_oneof![
        10=>(0_u8..6,0_u8..9,any::<bool>(),sequence_strategy()).prop_map(|(stage,acquire,strict,sequence)|Operation::Record{stage,acquire,strict,sequence}),
        4=>(axis_strategy(),0_u8..8).prop_map(|(axis,value)|Operation::Change{axis,value}),
        3=>sequence_strategy().prop_map(|sequence|Operation::Reobserve{sequence}),
        5=>(0_u8..3,0_u8..4,any::<bool>(),prop::option::of(axis_strategy()),any::<bool>(),any::<bool>(),0_u8..6).prop_map(|(slot,status,satisfied,drift,future,strict,stage)|Operation::ProbeEvidence{slot,status,satisfied,drift,future,strict,stage}),
        3=>proptest::sample::select(causes()).prop_map(|cause|Operation::Settle{cause}),
        3=>(any::<bool>(),any::<bool>()).prop_map(|(retain,clear_execution)|Operation::Restart{retain,clear_execution}),
        1=>Just(Operation::Read),
    ];
    let maximum = max_steps.min(256);
    let mixed = prop::collection::vec(operation.clone(), 0..=maximum)
        .prop_map(|operations| Case { operations });
    if maximum < 6 {
        return mixed.boxed();
    }
    let surrounding = (maximum - 6) / 2;
    let qualified_cancellation = (
        prop::collection::vec(operation.clone(), 0..=surrounding),
        0_u8..8,
        0_u8..4,
        any::<bool>(),
        prop::collection::vec(operation, 0..=surrounding),
    )
        .prop_map(|(mut prefix, content, execution, strict, suffix)| {
            prefix.extend([
                Operation::Restart { retain: false, clear_execution: false },
                Operation::Change { axis: Axis::Content, value: content },
                Operation::Change { axis: Axis::Execution, value: execution * 2 + 1 },
                Operation::Record { stage: 5, acquire: 7, strict, sequence: Sequence::Next },
                Operation::Reobserve { sequence: Sequence::Next },
                Operation::Settle { cause: Cause::Cancel },
            ]);
            prefix.extend(suffix);
            Case { operations: prefix }
        });
    prop_oneof![3 => mixed, 1 => qualified_cancellation].boxed()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Identity {
    run: u8,
    workspace: u8,
    content: u8,
    repository: u8,
    execution: Option<u8>,
    conversation: u64,
    sequence: u64,
}
impl Identity {
    fn actual(self) -> Result<Result<CandidateIdentity, SettlementError>, String> {
        let mut run = [1; 16];
        run[15] = self.run;
        let mut workspace = [2; 16];
        workspace[15] = self.workspace;
        Ok(CandidateIdentity::new(
            RunId::new(run).map_err(|error| format!("{error:?}"))?,
            WorkspaceId::new(workspace).map_err(|error| format!("{error:?}"))?,
            digest(self.content),
            digest(self.repository),
            self.execution.map(digest),
            self.conversation,
            self.sequence,
        ))
    }
    fn change(&mut self, axis: Axis, value: u8) {
        match axis {
            Axis::Run => self.run = value,
            Axis::Workspace => self.workspace = value,
            Axis::Content => self.content = value,
            Axis::Repository => self.repository = value,
            Axis::Execution => {
                self.execution = if value.is_multiple_of(2) { None } else { Some(value) };
            }
            Axis::Conversation => self.conversation = u64::from(value),
        }
    }
    fn same_source(self, other: Self) -> bool {
        self.run == other.run
            && self.workspace == other.workspace
            && self.content == other.content
            && self.conversation == other.conversation
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Evidence {
    status: u8,
    provenance: Identity,
    execution: bool,
    satisfied: bool,
}
impl Evidence {
    fn binds(self, candidate: Identity) -> bool {
        self.provenance.same_source(candidate)
            && (!self.execution
                || matches!((self.provenance.execution,candidate.execution),(Some(left),Some(right))if left==right))
    }
    fn positive(self, candidate: Identity) -> bool {
        self.status == 1
            && self.satisfied
            && self.binds(candidate)
            && self.provenance.sequence <= candidate.sequence
    }
    fn actual(self) -> Result<EvidenceStatus<QualificationEvidence>, String> {
        if self.status == 0 {
            return Ok(EvidenceStatus::Missing);
        }
        let provenance = self.provenance.actual()?.map_err(|error| error.to_string())?;
        let record = EvidenceRecord::new(
            provenance,
            if self.execution {
                EvidenceDependencies::SourceAndExecution
            } else {
                EvidenceDependencies::Source
            },
            if self.satisfied {
                QualificationEvidence::Satisfied
            } else {
                QualificationEvidence::Unsatisfied
            },
        );
        Ok(match self.status {
            1 => EvidenceStatus::Current(record),
            2 => EvidenceStatus::Failed(record),
            _ => EvidenceStatus::Stale(record),
        })
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Checkpoint {
    identity: Identity,
    stage: u8,
    evidence: [Evidence; 3],
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Disposition {
    Accepted,
    NoCandidateComplete,
    Candidate,
    Waiting,
    Failed,
    Cancelled,
    Recovery,
}
#[derive(Clone, Copy, Debug)]
struct Terminal {
    disposition: Disposition,
    cause: Cause,
    head: Option<Checkpoint>,
}
struct Model {
    selected: Identity,
    head: Option<Checkpoint>,
    remembered: Checkpoint,
    terminal: Option<Terminal>,
}
fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value; 32])
}
fn stage(value: u8) -> CandidateStage {
    match value {
        0 => CandidateStage::Observed,
        1 => CandidateStage::Changed,
        2 => CandidateStage::SelfChecked,
        3 => CandidateStage::GatesPassed,
        4 => CandidateStage::ReviewPending,
        _ => CandidateStage::Qualified,
    }
}
fn bump(metrics: &mut crate::Metrics, name: &str, value: u64) {
    *metrics.entry(name.into()).or_default() += value;
}
fn normalize(mut evidence: Evidence, identity: Identity) -> Result<Evidence, SettlementErrorKind> {
    if evidence.status == 0 {
        return Ok(evidence);
    }
    if !evidence.binds(identity) {
        evidence.status = 3;
        return Ok(evidence);
    }
    if evidence.provenance.sequence > identity.sequence {
        return Err(SettlementErrorKind::CurrentEvidenceBindingMismatch);
    }
    evidence.status = if evidence.satisfied { 1 } else { 2 };
    Ok(evidence)
}
fn predict(
    identity: Identity,
    requested: u8,
    mut evidence: [Evidence; 3],
    reconcile: bool,
) -> Result<Checkpoint, SettlementErrorKind> {
    if identity.sequence == 0 {
        return Err(SettlementErrorKind::ZeroCheckpointSequence);
    }
    if reconcile {
        for item in &mut evidence {
            *item = normalize(*item, identity)?;
        }
    }
    if evidence.iter().any(|item| {
        (item.status == 1 || item.status == 2)
            && (!item.binds(identity) || item.provenance.sequence > identity.sequence)
    }) {
        return Err(SettlementErrorKind::CurrentEvidenceBindingMismatch);
    }
    if evidence.iter().any(|item| item.status == 3 && item.binds(identity)) {
        return Err(SettlementErrorKind::StaleEvidenceBindingMismatch);
    }
    let [gates, obligations, review] = evidence.map(|item| item.positive(identity));
    let supported = if reconcile {
        match requested {
            0..=2 => requested,
            5 if gates && obligations && review => 5,
            _ if !gates => 2,
            4 if !review => 4,
            _ => 3,
        }
    } else {
        requested
    };
    let permitted = match supported {
        0..=2 => true,
        3 => gates,
        4 => gates && !review,
        _ => gates && obligations && review,
    };
    if !permitted {
        return Err(SettlementErrorKind::CandidateStageEvidenceMismatch);
    }
    Ok(Checkpoint { identity, stage: supported, evidence })
}
fn produce(
    identity: Identity,
    requested: u8,
    evidence: [Evidence; 3],
    reconcile: bool,
) -> Result<Result<CandidateCheckpoint, SettlementError>, String> {
    let identity = match identity.actual()? {
        Ok(identity) => identity,
        Err(error) => return Ok(Err(error)),
    };
    let [gates, obligations, review] = evidence.map(Evidence::actual);
    let (gates, obligations, review) = (gates?, obligations?, review?);
    Ok(if reconcile {
        CandidateCheckpoint::observe(identity, stage(requested), gates, obligations, review)
    } else {
        CandidateCheckpoint::new(identity, stage(requested), gates, obligations, review)
    })
}
fn compare_checkpoint(actual: &CandidateCheckpoint, expected: Checkpoint) -> Result<(), String> {
    let id = expected.identity.actual()?.map_err(|error| error.to_string())?;
    if actual.identity() != &id
        || actual.stage() != stage(expected.stage)
        || actual.is_qualified()
            != (expected.stage == 5
                && expected.evidence.iter().all(|item| item.positive(expected.identity)))
    {
        return Err(format!(
            "candidate identity/stage/qualification mismatch: actual {actual:?}, expected {expected:?}"
        ));
    }
    for (actual, expected) in
        [actual.gates(), actual.obligations(), actual.review()].into_iter().zip(expected.evidence)
    {
        if *actual != expected.actual()? {
            return Err("retained evidence provenance/status was rewritten".into());
        }
    }
    Ok(())
}
fn successor(current: Checkpoint, previous: Checkpoint) -> Result<(), SettlementErrorKind> {
    if current.identity.run != previous.identity.run
        || current.identity.workspace != previous.identity.workspace
    {
        return Err(SettlementErrorKind::CandidateLineageMismatch);
    }
    if current.identity.sequence <= previous.identity.sequence {
        return Err(SettlementErrorKind::CheckpointDidNotAdvance);
    }
    Ok(())
}
fn apply_candidate(
    expected: Result<Checkpoint, SettlementErrorKind>,
    actual: Result<CandidateCheckpoint, SettlementError>,
    model: &mut Model,
    reducer: &mut SettlementReducer,
    remembered: &mut CandidateCheckpoint,
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    let (expected, actual) = match (expected, actual) {
        (Err(expected), Err(actual)) if expected == actual.kind() => {
            bump(metrics, "checkpoint_construction_rejections", 1);
            return Ok(());
        }
        (Ok(expected), Ok(actual)) => (expected, actual),
        (expected, actual) => {
            return Err(format!("checkpoint construction {actual:?}, expected {expected:?}"));
        }
    };
    compare_checkpoint(&actual, expected)?;
    let admission = if model.terminal.is_some() {
        Err(SettlementErrorKind::AlreadySettled)
    } else if let Some(previous) = model.head {
        successor(expected, previous)
    } else {
        Ok(())
    };
    match (admission, reducer.observe(actual)) {
        (Err(expected), Err(actual)) if expected == actual.kind() => {
            bump(metrics, "checkpoint_transition_rejections", 1);
        }
        (Ok(()), Ok(())) => {
            if expected.stage < model.remembered.stage {
                bump(metrics, "qualification_downgrades", 1);
            }
            model.head = Some(expected);
            model.remembered = expected;
            *remembered = actual;
            model.selected.sequence = expected.identity.sequence;
            bump(metrics, "accepted_checkpoints", 1);
            if expected.stage == 5 {
                bump(metrics, "qualified_checkpoints", 1);
            }
        }
        (expected, actual) => {
            return Err(format!("checkpoint admission {actual:?}, expected {expected:?}"));
        }
    }
    Ok(())
}
fn disposition(cause: Cause, head: Option<Checkpoint>) -> Disposition {
    match cause {
        Cause::NoChange if head.is_none() => Disposition::NoCandidateComplete,
        Cause::NoChange => Disposition::Candidate,
        Cause::Wait => Disposition::Waiting,
        Cause::Cancel => Disposition::Cancelled,
        Cause::Recovery => Disposition::Recovery,
        _ if head.is_some_and(|value| value.stage == 5) => Disposition::Accepted,
        _ if head.is_some() => Disposition::Candidate,
        _ => Disposition::Failed,
    }
}
fn actual_disposition(value: Disposition) -> RunDisposition {
    match value {
        Disposition::Accepted => RunDisposition::Accepted,
        Disposition::NoCandidateComplete => RunDisposition::CompletedWithoutCandidate,
        Disposition::Candidate => RunDisposition::CandidateAvailable,
        Disposition::Waiting => RunDisposition::WaitingForUser,
        Disposition::Failed => RunDisposition::FailedNoCandidate,
        Disposition::Cancelled => RunDisposition::Cancelled,
        Disposition::Recovery => RunDisposition::RecoveryRequired,
    }
}
fn compare_terminal(actual: &RunSettlement, expected: Terminal) -> Result<(), String> {
    if actual.disposition() != actual_disposition(expected.disposition)
        || actual.cause() != expected.cause.actual()
        || actual.is_accepted() != (expected.disposition == Disposition::Accepted)
    {
        return Err(format!("terminal disposition changed: {actual:?}, expected {expected:?}"));
    }
    match (actual.checkpoint(), expected.head) {
        (Some(actual), Some(expected)) => compare_checkpoint(actual, expected),
        (None, None) => Ok(()),
        _ => Err("terminal lost or fabricated candidate".into()),
    }
}
fn compare(
    model: &Model,
    reducer: &SettlementReducer,
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    match (reducer.checkpoint(), model.head) {
        (Some(actual), Some(expected)) => compare_checkpoint(actual, expected)?,
        (None, None) => {}
        _ => return Err("reducer lost/fabricated retained candidate".into()),
    }
    match (reducer.terminal(), model.terminal) {
        (Some(actual), Some(expected)) => compare_terminal(actual, expected)?,
        (None, None) => {}
        _ => return Err("reducer terminal state diverged".into()),
    }
    bump(metrics, "state_comparisons", 1);
    Ok(())
}
fn next_identity(model: &Model, sequence: Sequence) -> Identity {
    let mut identity = model.selected;
    identity.sequence = match sequence {
        Sequence::Next => model.remembered.identity.sequence + 1,
        Sequence::Same => model.remembered.identity.sequence,
        Sequence::Backwards => model.remembered.identity.sequence.saturating_sub(1),
    };
    identity
}
fn acquire(input: &mut [Evidence; 3], identity: Identity, kind: u8) {
    let assignments: Vec<_> = match kind {
        1 => vec![(0, true)],
        2 => vec![(0, false)],
        3 => vec![(1, true)],
        4 => vec![(1, false)],
        5 => vec![(2, true)],
        6 => vec![(2, false)],
        7 => (0..3).map(|index| (index, true)).collect(),
        8 => (0..3).map(|index| (index, false)).collect(),
        _ => Vec::new(),
    };
    for (index, satisfied) in assignments {
        input[index] = Evidence {
            status: if satisfied { 1 } else { 2 },
            provenance: identity,
            execution: index == 0,
            satisfied,
        };
    }
}

pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    if case.operations.len() > 512 {
        return Err("settlement history exceeds discovery bound".into());
    }
    let initial = Identity {
        run: 1,
        workspace: 1,
        content: 1,
        repository: 1,
        execution: Some(1),
        conversation: 1,
        sequence: 1,
    };
    let missing = Evidence { status: 0, provenance: initial, execution: false, satisfied: false };
    let baseline = Checkpoint { identity: initial, stage: 0, evidence: [missing; 3] };
    let mut remembered =
        produce(initial, 0, [missing; 3], true)?.map_err(|error| error.to_string())?;
    let mut model = Model { selected: initial, head: None, remembered: baseline, terminal: None };
    let mut reducer = SettlementReducer::new();
    let mut metrics = crate::Metrics::new();
    compare(&model, &reducer, &mut metrics)?;
    for (step, operation) in case.operations.iter().enumerate() {
        let result = (|| {
            match *operation {
                Operation::Record { stage, acquire: kind, strict, sequence } => {
                    if stage >= 6 || kind >= 9 {
                        return Err("invalid stage/acquisition".into());
                    }
                    let identity = next_identity(&model, sequence);
                    let mut evidence = model.remembered.evidence;
                    acquire(&mut evidence, identity, kind);
                    apply_candidate(
                        predict(identity, stage, evidence, !strict),
                        produce(identity, stage, evidence, !strict)?,
                        &mut model,
                        &mut reducer,
                        &mut remembered,
                        &mut metrics,
                    )?;
                    bump(
                        &mut metrics,
                        if strict {
                            "strict_checkpoint_calls"
                        } else {
                            "reconciled_checkpoint_calls"
                        },
                        1,
                    );
                }
                Operation::Change { axis, value } => {
                    model.selected.change(axis, value);
                    bump(&mut metrics, "candidate_axis_updates", 1);
                }
                Operation::Reobserve { sequence } => {
                    let identity = next_identity(&model, sequence);
                    let requested = if model.remembered.identity.same_source(identity) {
                        model.remembered.stage
                    } else {
                        1
                    };
                    let expected = predict(identity, requested, model.remembered.evidence, true)
                        .and_then(|checkpoint| {
                            successor(checkpoint, model.remembered)?;
                            Ok(checkpoint)
                        });
                    let actual = match identity.actual()? {
                        Ok(identity) => remembered.reobserve(identity),
                        Err(error) => Err(error),
                    };
                    apply_candidate(
                        expected,
                        actual,
                        &mut model,
                        &mut reducer,
                        &mut remembered,
                        &mut metrics,
                    )?;
                    bump(&mut metrics, "reobserved_checkpoints", 1);
                }
                Operation::ProbeEvidence {
                    slot,
                    status,
                    satisfied,
                    drift,
                    future,
                    strict,
                    stage,
                } => {
                    if slot >= 3 || status >= 4 || stage >= 6 {
                        return Err("invalid evidence probe bounds".into());
                    }
                    let identity = next_identity(&model, Sequence::Next);
                    let mut provenance = identity;
                    if let Some(axis) = drift {
                        let value = match axis {
                            Axis::Run => provenance.run.wrapping_add(1),
                            Axis::Workspace => provenance.workspace.wrapping_add(1),
                            Axis::Content => provenance.content.wrapping_add(1),
                            Axis::Repository => provenance.repository.wrapping_add(1),
                            Axis::Execution => {
                                provenance.execution.unwrap_or(0).wrapping_add(2) | 1
                            }
                            Axis::Conversation => {
                                u8::try_from(provenance.conversation).unwrap_or(0).wrapping_add(1)
                            }
                        };
                        provenance.change(axis, value);
                    }
                    if future {
                        provenance.sequence = identity.sequence + 1000;
                    }
                    let mut evidence = model.remembered.evidence;
                    evidence[usize::from(slot)] =
                        Evidence { status, provenance, execution: slot == 0, satisfied };
                    apply_candidate(
                        predict(identity, stage, evidence, !strict),
                        produce(identity, stage, evidence, !strict)?,
                        &mut model,
                        &mut reducer,
                        &mut remembered,
                        &mut metrics,
                    )?;
                    bump(&mut metrics, "evidence_binding_probes", 1);
                    if future {
                        bump(&mut metrics, "future_evidence_probes", 1);
                    }
                }
                Operation::Settle { cause } => {
                    if model.terminal.is_none() && model.head.is_some_and(|head| head.stage == 5) {
                        bump(&mut metrics, "terminal_attempts_after_qualification", 1);
                        if matches!(cause, Cause::Cancel) {
                            bump(&mut metrics, "qualified_cancellation_attempts", 1);
                        }
                    }
                    let actual = reducer.settle(cause.actual());
                    if model.terminal.is_some() {
                        if !matches!(actual,Err(error)if error.kind()==SettlementErrorKind::AlreadySettled)
                        {
                            return Err("terminal reducer accepted a second settlement".into());
                        }
                        bump(&mut metrics, "repeated_terminal_rejections", 1);
                    } else {
                        let terminal = Terminal {
                            disposition: disposition(cause, model.head),
                            cause,
                            head: model.head,
                        };
                        let actual = actual.map_err(|error| error.to_string())?;
                        compare_terminal(&actual, terminal)?;
                        model.terminal = Some(terminal);
                        bump(&mut metrics, &format!("terminal_{:?}", terminal.disposition), 1);
                    }
                }
                Operation::Restart { retain, clear_execution } => {
                    reducer = SettlementReducer::new();
                    model.terminal = None;
                    if retain {
                        if let Some(head) = model.head {
                            reducer.observe(remembered).map_err(|error| error.to_string())?;
                            compare_checkpoint(&remembered, head)?;
                        }
                    } else {
                        model.head = None;
                    }
                    if clear_execution {
                        model.selected.execution = None;
                    }
                    bump(&mut metrics, "new_reducer_instances", 1);
                }
                Operation::Read => bump(&mut metrics, "explicit_reads", 1),
            }
            compare(&model, &reducer, &mut metrics)
        })();
        result.map_err(|error| format!("step {step} {operation:?}: {error}"))?;
        bump(&mut metrics, "operations", 1);
    }
    bump(&mut metrics, "cases", 1);
    Ok(metrics)
}

pub fn calibration_case() -> Case {
    Case {
        operations: vec![
            Operation::Record { stage: 5, acquire: 7, strict: false, sequence: Sequence::Next },
            Operation::Settle { cause: Cause::Cancel },
        ],
    }
}
pub fn examples() -> Vec<Case> {
    let mut cases = vec![calibration_case()];
    for cause in causes() {
        for qualified in [false, true] {
            let mut operations = Vec::new();
            if qualified {
                operations.push(Operation::Record {
                    stage: 5,
                    acquire: 7,
                    strict: false,
                    sequence: Sequence::Next,
                });
            }
            operations.extend([
                Operation::Settle { cause },
                Operation::Settle { cause: Cause::Completed },
                Operation::Record { stage: 0, acquire: 0, strict: false, sequence: Sequence::Next },
                Operation::Read,
            ]);
            cases.push(Case { operations });
        }
    }
    let mut axes = Vec::new();
    for axis in [
        Axis::Repository,
        Axis::Execution,
        Axis::Content,
        Axis::Conversation,
        Axis::Run,
        Axis::Workspace,
    ] {
        axes.extend([
            Operation::Restart { retain: false, clear_execution: false },
            Operation::Change { axis, value: 1 },
            Operation::Record { stage: 5, acquire: 7, strict: false, sequence: Sequence::Next },
            Operation::Change { axis, value: 3 },
            Operation::Reobserve { sequence: Sequence::Next },
            Operation::Change { axis, value: 1 },
            Operation::Reobserve { sequence: Sequence::Next },
        ]);
    }
    cases.push(Case { operations: axes });
    let mut truth_table = Vec::new();
    for requested in 0..6 {
        for bitmask in 0..8 {
            truth_table.push(Operation::Restart { retain: false, clear_execution: false });
            truth_table.push(Operation::Record {
                stage: 0,
                acquire: 8,
                strict: false,
                sequence: Sequence::Next,
            });
            for slot in 0..3 {
                let kind = if bitmask & (1 << slot) != 0 { 1 + slot * 2 } else { 2 + slot * 2 };
                truth_table.push(Operation::Record {
                    stage: 0,
                    acquire: kind,
                    strict: false,
                    sequence: Sequence::Next,
                });
            }
            truth_table.push(Operation::Record {
                stage: requested,
                acquire: 0,
                strict: false,
                sequence: Sequence::Next,
            });
        }
    }
    cases.push(Case { operations: truth_table });
    let mut invalid = Vec::new();
    for strict in [false, true] {
        for slot in 0..3 {
            for status in 0..4 {
                invalid.push(Operation::ProbeEvidence {
                    slot,
                    status,
                    satisfied: true,
                    drift: None,
                    future: true,
                    strict,
                    stage: 5,
                });
                invalid.push(Operation::ProbeEvidence {
                    slot,
                    status,
                    satisfied: false,
                    drift: Some(Axis::Content),
                    future: false,
                    strict,
                    stage: 0,
                });
            }
        }
    }
    invalid.extend([
        Operation::Record { stage: 0, acquire: 0, strict: true, sequence: Sequence::Same },
        Operation::Record { stage: 0, acquire: 0, strict: false, sequence: Sequence::Backwards },
        Operation::Restart { retain: true, clear_execution: true },
        Operation::Reobserve { sequence: Sequence::Next },
    ]);
    cases.push(Case { operations: invalid });
    cases
}

//! Parent-map, causal-message and join oracle over real durable collaboration transitions.
use peritus_collaboration::{
    ArtifactHandoff, CollaborationBinding, CollaborationCommand,
    CollaborationCommandKind as Command, CollaborationErrorKind as Error, CollaborationId,
    CollaborationLimits, CollaborationMessage, CollaborationMessageId, CollaborationPhase,
    CollaborationState, CollaborationTaskId, CollaborationTerminalKind, Delegation, JoinPolicy,
    ReservationObservation, TaskPhase, TaskTerminal, TaskTerminalKind,
    commit_collaboration_transition, decide, load_collaboration_replay, start,
};
use peritus_journal::{SqliteJournal, SqliteJournalOptions, StoreId};
use peritus_role::HarnessRole;
use peritus_scheduler::{DispatchId, SchedulerId, WorkId};
use peritus_types::{
    AcceptanceSpecId, ActorId, ArtifactId, CommandId, EventId, Generation, HarnessId, PolicyId,
    ProviderProfileId, RevisionNumber, RevisionTuple, RunId, Sha256Digest, WorkspaceId,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub root_join: u8,
    pub depth: u16,
    pub fan_out: u16,
    pub recipients: u16,
    pub artifacts: u16,
    pub operations: Vec<Operation>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    Offer {
        task: u8,
        parent: u8,
        owner: u8,
        required: bool,
        join: u8,
        correct_owner: bool,
    },
    Accept {
        task: u8,
        correct_owner: bool,
    },
    Reject {
        task: u8,
        correct_owner: bool,
    },
    Activate {
        task: u8,
        correct_binding: bool,
    },
    Send {
        id: u8,
        task: u8,
        reverse: bool,
        correct_sender: bool,
        stale: bool,
        payload: u16,
        artifact: u8,
    },
    Acknowledge {
        id: u8,
        correct_receiver: bool,
    },
    Complete {
        task: u8,
        success: bool,
        correct_owner: bool,
        artifact: u8,
    },
    Abandon {
        task: u8,
        correct_owner: bool,
    },
    Cancel {
        task: u8,
        requester: u8,
    },
    AcknowledgeCancellation {
        task: u8,
        correct_owner: bool,
    },
    Pause {
        paused: bool,
        correct_owner: bool,
    },
    Finalize,
    StaleFence,
    ReplayLast,
    Reopen,
    Read,
}
fn operation() -> BoxedStrategy<Operation> {
    prop_oneof![
    6=>(1_u8..16,0_u8..16,0_u8..4,any::<bool>(),0_u8..3,any::<bool>()).prop_map(|(task,parent,owner,required,join,correct_owner)|Operation::Offer{task,parent,owner,required,join,correct_owner}),
    4=>(0_u8..16,any::<bool>()).prop_map(|(task,correct_owner)|Operation::Accept{task,correct_owner}),
    2=>(0_u8..16,any::<bool>()).prop_map(|(task,correct_owner)|Operation::Reject{task,correct_owner}),
    5=>(0_u8..16,any::<bool>()).prop_map(|(task,correct_binding)|Operation::Activate{task,correct_binding}),
    7=>(0_u8..64,0_u8..16,any::<bool>(),any::<bool>(),any::<bool>(),1_u16..1026,0_u8..3).prop_map(|(id,task,reverse,correct_sender,stale,payload,artifact)|Operation::Send{id,task,reverse,correct_sender,stale,payload,artifact}),
    3=>(0_u8..64,any::<bool>()).prop_map(|(id,correct_receiver)|Operation::Acknowledge{id,correct_receiver}),
    4=>(0_u8..16,any::<bool>(),any::<bool>(),0_u8..3).prop_map(|(task,success,correct_owner,artifact)|Operation::Complete{task,success,correct_owner,artifact}),
    2=>(0_u8..16,any::<bool>()).prop_map(|(task,correct_owner)|Operation::Abandon{task,correct_owner}),
    3=>(0_u8..16,0_u8..3).prop_map(|(task,requester)|Operation::Cancel{task,requester}),
    3=>(0_u8..16,any::<bool>()).prop_map(|(task,correct_owner)|Operation::AcknowledgeCancellation{task,correct_owner}),
    2=>(any::<bool>(),any::<bool>()).prop_map(|(paused,correct_owner)|Operation::Pause{paused,correct_owner}),
    1=>Just(Operation::Finalize),1=>Just(Operation::StaleFence),1=>Just(Operation::ReplayLast),2=>Just(Operation::Reopen),1=>Just(Operation::Read),
].boxed()
}
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let maximum = max_steps.min(128);
    let mixed =
        (
            0_u8..3,
            1_u16..5,
            1_u16..5,
            1_u16..3,
            1_u16..5,
            prop::collection::vec(operation(), 0..=maximum),
        )
            .prop_map(|(root_join, depth, fan_out, recipients, artifacts, operations)| {
                Case { root_join, depth, fan_out, recipients, artifacts, operations }
            });
    if maximum < 32 {
        return mixed.boxed();
    }
    let permutations = vec![[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
    let causal = (
        proptest::sample::select(permutations.clone()),
        proptest::sample::select(permutations),
        any::<bool>(),
        any::<bool>(),
        prop::collection::vec(operation(), 0..=maximum - 32),
    )
        .prop_map(|(first, second, reverse, leave_active, mut suffix)| {
            let mut case = causal_case(first, second, reverse);
            if leave_active {
                case.operations.truncate(22);
            }
            case.operations.append(&mut suffix);
            case
        });
    let cancellation =
        (any::<bool>(), 0_u8..2, prop::collection::vec(operation(), 0..=maximum - 32)).prop_map(
            |(required, root_join, mut suffix)| {
                let mut case = calibration();
                case.root_join = root_join + 1;
                if let Operation::Offer { required: row, .. } = &mut case.operations[1] {
                    *row = required;
                }
                case.operations.append(&mut suffix);
                case
            },
        );
    prop_oneof![2=>mixed,1=>causal,1=>cancellation].boxed()
}
fn bytes(value: u64) -> [u8; 16] {
    u128::from(value + 1).to_be_bytes()
}
fn debug(error: impl std::fmt::Debug) -> String {
    format!("{error:?}")
}
fn digest(value: u8) -> Sha256Digest {
    Sha256Digest::new([value.wrapping_add(1); 32])
}
fn task(value: u8) -> Result<CollaborationTaskId, String> {
    CollaborationTaskId::new(bytes(u64::from(value))).map_err(debug)
}
fn message(value: u8) -> Result<CollaborationMessageId, String> {
    CollaborationMessageId::new(bytes(u64::from(value))).map_err(debug)
}
fn actor(value: u8) -> Result<ActorId, String> {
    ActorId::new(bytes(u64::from(value))).map_err(debug)
}
fn work(value: u8) -> Result<WorkId, String> {
    WorkId::new(bytes(u64::from(value))).map_err(debug)
}
fn revision(different: bool) -> Result<RevisionTuple, String> {
    Ok(RevisionTuple::new(
        AcceptanceSpecId::new(bytes(0)).map_err(debug)?,
        HarnessId::new(bytes(0)).map_err(debug)?,
        WorkspaceId::new(bytes(u64::from(different))).map_err(debug)?,
        Generation::first(),
        RevisionNumber::first(),
        PolicyId::new(bytes(0)).map_err(debug)?,
        ProviderProfileId::new(bytes(0)).map_err(debug)?,
    ))
}
fn join(value: u8) -> JoinPolicy {
    match value {
        0 => JoinPolicy::NoChildren,
        1 => JoinPolicy::AllRequired,
        _ => JoinPolicy::AnyRequired,
    }
}
fn limits(case: &Case) -> Result<CollaborationLimits, String> {
    CollaborationLimits::new(
        24,
        case.depth,
        case.fan_out,
        32,
        case.recipients,
        1024,
        case.artifacts,
        1_048_576,
        1_048_576,
    )
    .map_err(debug)
}
fn handoff(id: u8, kind: u8) -> Result<Option<ArtifactHandoff>, String> {
    if kind == 0 {
        return Ok(None);
    }
    Ok(Some(
        ArtifactHandoff::new(
            ArtifactId::new(bytes(u64::from(id))).map_err(debug)?,
            digest(id),
            digest(id.wrapping_add(1)),
            revision(kind == 2)?,
        )
        .map_err(debug)?,
    ))
}
fn terminal(kind: u8, id: u8, artifact: u8) -> Result<TaskTerminal, String> {
    TaskTerminal::new(
        match kind {
            0 => TaskTerminalKind::Succeeded,
            1 => TaskTerminalKind::Failed,
            2 => TaskTerminalKind::Rejected,
            3 => TaskTerminalKind::Cancelled,
            _ => TaskTerminalKind::Abandoned,
        },
        if kind == 0 { handoff(id, artifact)? } else { None },
        if kind == 3 { Sha256Digest::new([0; 32]) } else { digest(id) },
    )
    .map_err(debug)
}
fn bump(metrics: &mut crate::Metrics, name: &str) {
    *metrics.entry(name.into()).or_default() += 1;
}

#[derive(Clone, Debug)]
struct Task {
    parent: Option<u8>,
    depth: u16,
    owner: u8,
    required: bool,
    join: u8,
    phase: u8,
    reservation: bool,
    end: Option<(u8, u8, Option<u8>)>,
}
#[derive(Clone, Debug)]
struct Message {
    task: u8,
    sender: u8,
    receiver: u8,
    ordinal: u32,
    predecessor: Option<u8>,
    payload: u16,
    artifact: u8,
    acknowledged: bool,
}
#[derive(Clone)]
struct Model {
    tasks: BTreeMap<u8, Task>,
    messages: BTreeMap<u8, Message>,
    order: BTreeMap<u8, Vec<u8>>,
    paused: bool,
    terminal: Option<u8>,
    sequence: u64,
}
impl Model {
    fn new(root_join: u8) -> Self {
        Self {
            tasks: BTreeMap::from([(
                0,
                Task {
                    parent: None,
                    depth: 0,
                    owner: 0,
                    required: true,
                    join: root_join,
                    phase: 1,
                    reservation: false,
                    end: None,
                },
            )]),
            messages: BTreeMap::new(),
            order: BTreeMap::new(),
            paused: false,
            terminal: None,
            sequence: 1,
        }
    }
    fn owner(&self, id: u8) -> u8 {
        self.tasks.get(&id).map_or(0, |task| task.owner)
    }
    fn acting(&self, id: u8, correct: bool) -> u8 {
        self.owner(id) + if correct { 0 } else { 8 }
    }
    fn parent_owner(&self, id: u8) -> u8 {
        self.tasks.get(&id).and_then(|task| task.parent).map_or(0, |parent| self.owner(parent))
    }
    fn children(&self, id: u8) -> Vec<u8> {
        self.tasks.iter().filter(|(_, task)| task.parent == Some(id)).map(|(id, _)| *id).collect()
    }
    fn descendants(&self, id: u8) -> BTreeSet<u8> {
        let mut result = BTreeSet::new();
        let mut frontier = vec![id];
        while let Some(parent) = frontier.pop() {
            for child in self.children(parent) {
                if result.insert(child) {
                    frontier.push(child);
                }
            }
        }
        result
    }
    fn joined(&self, id: u8) -> bool {
        let Some(task) = self.tasks.get(&id) else {
            return false;
        };
        let required =
            self.children(id).into_iter().filter(|id| self.tasks[id].required).collect::<Vec<_>>();
        let successes =
            required.iter().filter(|id| matches!(self.tasks[*id].end, Some((0, _, _)))).count();
        match task.join {
            0 => required.is_empty(),
            1 => successes == required.len(),
            _ => successes > 0,
        }
    }
    fn pending(&self) -> bool {
        self.messages.values().any(|message| !message.acknowledged)
            || self.tasks.values().any(|task| task.phase == 3)
    }
    fn owned(&self, id: u8, correct: bool) -> Result<&Task, Error> {
        let row = self.tasks.get(&id).ok_or(Error::UnknownIdentity)?;
        if !correct {
            return Err(Error::OwnerMismatch);
        }
        Ok(row)
    }
    fn message_fields(
        &self,
        id: u8,
        reverse: bool,
        correct: bool,
        stale: bool,
    ) -> (u8, u8, u32, Option<u8>) {
        let order = self.order.get(&id).map_or(&[][..], Vec::as_slice);
        let (owner, parent) = (self.owner(id), self.parent_owner(id));
        let (mut sender, receiver) = if reverse { (parent, owner) } else { (owner, parent) };
        if !correct {
            sender += 8;
        }
        let (ordinal, predecessor) = if stale {
            if order.is_empty() {
                (2, Some(250))
            } else {
                (u32::try_from(order.len()).unwrap_or(0) + 1, Some(250))
            }
        } else {
            (u32::try_from(order.len()).unwrap_or(0) + 1, order.last().copied())
        };
        (sender, receiver, ordinal, predecessor)
    }
    fn apply(
        &mut self,
        case: &Case,
        operation: &Operation,
        metrics: &mut crate::Metrics,
    ) -> Result<(), Error> {
        if let Operation::Offer { task, parent, .. } = operation
            && task == parent
        {
            return Err(Error::CausalityViolation);
        }
        if self.terminal.is_some() {
            return Err(Error::IllegalTransition);
        }
        if matches!(operation, Operation::StaleFence) {
            return Err(Error::StaleFence);
        }
        match operation {
            Operation::Offer { task: id, parent, owner, required, join, correct_owner } => {
                if self.paused {
                    return Err(Error::IllegalTransition);
                }
                if self.tasks.contains_key(id) {
                    return Err(Error::IdentityConflict);
                }
                if self.tasks.len() >= 24 {
                    return Err(Error::LimitExceeded);
                }
                let p = self.tasks.get(parent).ok_or(Error::UnknownIdentity)?;
                if p.phase != 2 || !*correct_owner || p.depth + 1 > case.depth || p.join == 0 {
                    return Err(Error::CausalityViolation);
                }
                if self.children(*parent).len() >= usize::from(case.fan_out) {
                    return Err(Error::LimitExceeded);
                }
                self.tasks.insert(
                    *id,
                    Task {
                        parent: Some(*parent),
                        depth: p.depth + 1,
                        owner: *owner,
                        required: *required,
                        join: *join,
                        phase: 0,
                        reservation: false,
                        end: None,
                    },
                );
                bump(metrics, "accepted_delegations");
            }
            Operation::Accept { task: id, correct_owner } => {
                if self.owned(*id, *correct_owner)?.phase != 0 {
                    return Err(Error::IllegalTransition);
                }
                self.tasks.get_mut(id).ok_or(Error::UnknownIdentity)?.phase = 1;
                bump(metrics, "accepted_ownership");
            }
            Operation::Reject { task: id, correct_owner } => {
                if self.owned(*id, *correct_owner)?.phase != 0 {
                    return Err(Error::IllegalTransition);
                }
                let row = self.tasks.get_mut(id).ok_or(Error::UnknownIdentity)?;
                row.phase = 4;
                row.end = Some((2, 0, None));
            }
            Operation::Activate { task: id, correct_binding } => {
                let row = self.tasks.get_mut(id).ok_or(Error::UnknownIdentity)?;
                if row.phase != 1 || !*correct_binding {
                    return Err(Error::BindingMismatch);
                }
                row.phase = 2;
                row.reservation = true;
                bump(metrics, "active_tasks");
            }
            Operation::Send {
                id,
                task: id_task,
                reverse,
                correct_sender,
                stale,
                payload,
                artifact,
            } => {
                if self.messages.len() >= 32 || self.messages.contains_key(id) || *payload > 1024 {
                    return Err(Error::LimitExceeded);
                }
                let row = self.tasks.get(id_task).ok_or(Error::UnknownIdentity)?;
                if row.phase != 2 {
                    return Err(Error::IllegalTransition);
                }
                if !*correct_sender {
                    return Err(Error::OwnerMismatch);
                }
                if *artifact == 2 {
                    return Err(Error::BindingMismatch);
                }
                let order = self.order.get(id_task).map_or(&[][..], Vec::as_slice);
                if *artifact > 0
                    && order.iter().filter(|id| self.messages[*id].artifact > 0).count()
                        >= usize::from(case.artifacts)
                {
                    return Err(Error::LimitExceeded);
                }
                if *stale {
                    return Err(Error::CausalityViolation);
                }
                let (sender, receiver, ordinal, predecessor) =
                    self.message_fields(*id_task, *reverse, true, false);
                let mut recipients =
                    order.iter().map(|id| self.messages[id].receiver).collect::<BTreeSet<_>>();
                recipients.insert(receiver);
                if recipients.len() > usize::from(case.recipients) {
                    return Err(Error::LimitExceeded);
                }
                if predecessor.is_some_and(|previous| previous > *id) {
                    bump(metrics, "descending_causal_identity_edges");
                }
                self.messages.insert(
                    *id,
                    Message {
                        task: *id_task,
                        sender,
                        receiver,
                        ordinal,
                        predecessor,
                        payload: *payload,
                        artifact: *artifact,
                        acknowledged: false,
                    },
                );
                self.order.entry(*id_task).or_default().push(*id);
                bump(metrics, "accepted_causal_messages");
            }
            Operation::Acknowledge { id, correct_receiver } => {
                let message = self.messages.get_mut(id).ok_or(Error::UnknownIdentity)?;
                if message.acknowledged || !*correct_receiver {
                    return Err(Error::OwnerMismatch);
                }
                message.acknowledged = true;
                bump(metrics, "acknowledged_messages");
            }
            Operation::Complete { task: id, success, correct_owner, artifact } => {
                if self.owned(*id, *correct_owner)?.phase != 2 {
                    return Err(Error::IllegalTransition);
                }
                if *success && *artifact == 2 {
                    return Err(Error::BindingMismatch);
                }
                if *success
                    && *artifact > 0
                    && self
                        .messages
                        .values()
                        .filter(|message| message.task == *id && message.artifact > 0)
                        .count()
                        >= usize::from(case.artifacts)
                {
                    return Err(Error::LimitExceeded);
                }
                if *success && !self.joined(*id) {
                    return Err(Error::JoinUnsatisfied);
                }
                let row = self.tasks.get_mut(id).ok_or(Error::UnknownIdentity)?;
                row.phase = 4;
                row.end = Some((u8::from(!success), if *success { *artifact } else { 0 }, None));
                bump(metrics, "completed_tasks");
            }
            Operation::Abandon { task: id, correct_owner } => {
                if !matches!(self.owned(*id, *correct_owner)?.phase, 1 | 2) {
                    return Err(Error::IllegalTransition);
                }
                let row = self.tasks.get_mut(id).ok_or(Error::UnknownIdentity)?;
                row.phase = 4;
                row.end = Some((4, 0, None));
            }
            Operation::Cancel { task: id, requester } => {
                let row = self.tasks.get(id).ok_or(Error::UnknownIdentity)?;
                if row.phase == 4 {
                    return Err(Error::IllegalTransition);
                }
                if *requester >= 2 {
                    return Err(Error::OwnerMismatch);
                }
                let root = *id;
                let mut affected = self.descendants(root);
                affected.insert(root);
                for id in affected {
                    let row = self.tasks.get_mut(&id).ok_or(Error::UnknownIdentity)?;
                    if row.phase == 4 {
                        continue;
                    }
                    if matches!(row.phase, 2 | 3) {
                        row.phase = 3;
                    } else {
                        row.phase = 4;
                        row.end = Some((3, 0, Some(root)));
                    }
                    bump(metrics, "propagated_cancellation_members");
                    if id != root {
                        bump(metrics, "descendant_cancellation_members");
                    }
                }
                bump(metrics, "accepted_cancellations");
            }
            Operation::AcknowledgeCancellation { task: id, correct_owner } => {
                if self.owned(*id, *correct_owner)?.phase != 3 {
                    return Err(Error::IllegalTransition);
                }
                let row = self.tasks.get_mut(id).ok_or(Error::UnknownIdentity)?;
                row.phase = 4;
                row.end = Some((3, 0, None));
                bump(metrics, "cancellation_acknowledgements");
            }
            Operation::Pause { paused, correct_owner } => {
                if !*correct_owner {
                    return Err(Error::OwnerMismatch);
                }
                if self.paused == *paused {
                    return Err(Error::IllegalTransition);
                }
                self.paused = *paused;
            }
            Operation::Finalize => {
                if self.pending() || self.tasks.values().any(|task| task.phase != 4) {
                    return Err(Error::JoinUnsatisfied);
                }
                let root =
                    self.tasks.get(&0).and_then(|task| task.end).ok_or(Error::JoinUnsatisfied)?;
                self.terminal = Some(match root.0 {
                    0 if self.joined(0) => 0,
                    0 => 5,
                    1 | 2 => 1,
                    3 => 2,
                    _ => 3,
                });
                bump(metrics, "finalized_aggregates");
            }
            Operation::ReplayLast | Operation::Reopen | Operation::Read => return Ok(()),
            Operation::StaleFence => return Err(Error::StaleFence),
        }
        self.sequence += 1;
        Ok(())
    }
}

fn assignment(model: &Model, id: u8, row: &Task) -> Result<Delegation, String> {
    if let Some(parent) = row.parent {
        Delegation::child(
            task(id)?,
            task(0)?,
            task(parent)?,
            row.depth,
            actor(row.owner)?,
            HarnessRole::Reviewer,
            actor(model.owner(parent))?,
            work(id)?,
            digest(id),
            row.required,
            join(row.join),
        )
        .map_err(debug)
    } else {
        Delegation::root(
            task(id)?,
            actor(row.owner)?,
            HarnessRole::Writer,
            work(id)?,
            digest(id),
            join(row.join),
        )
        .map_err(debug)
    }
}
fn actual_kind(model: &Model, operation: &Operation) -> Result<Command, String> {
    Ok(match operation {
        Operation::Offer { task: id, parent, owner, required, join, correct_owner } => {
            let depth = model.tasks.get(parent).map_or(1, |row| row.depth + 1);
            let offered = model.acting(*parent, *correct_owner);
            Command::OfferDelegation {
                offered_by: actor(offered)?,
                assignment: Delegation::child(
                    task(*id)?,
                    task(0)?,
                    task(*parent)?,
                    depth,
                    actor(*owner)?,
                    HarnessRole::Reviewer,
                    actor(offered)?,
                    work(*id)?,
                    digest(*id),
                    *required,
                    self::join(*join),
                )
                .map_err(debug)?,
            }
        }
        Operation::Accept { task: id, correct_owner } => Command::AcceptDelegation {
            task_id: task(*id)?,
            accepted_by: actor(model.acting(*id, *correct_owner))?,
        },
        Operation::Reject { task: id, correct_owner } => Command::RejectDelegation {
            task_id: task(*id)?,
            rejected_by: actor(model.acting(*id, *correct_owner))?,
            reason_digest: digest(*id),
        },
        Operation::Activate { task: id, correct_binding } => Command::ActivateTask {
            task_id: task(*id)?,
            observation: ReservationObservation::new(
                work(*id)?,
                DispatchId::new(bytes(u64::from(*id))).map_err(debug)?,
                actor(model.acting(*id, *correct_binding))?,
                revision(false)?,
            ),
        },
        Operation::Send {
            id,
            task: id_task,
            reverse,
            correct_sender,
            stale,
            payload,
            artifact,
        } => {
            let (sender, receiver, ordinal, predecessor) =
                model.message_fields(*id_task, *reverse, *correct_sender, *stale);
            Command::SendMessage {
                message: CollaborationMessage::new(
                    message(*id)?,
                    task(0)?,
                    task(*id_task)?,
                    actor(sender)?,
                    actor(receiver)?,
                    ordinal,
                    predecessor.map(message).transpose()?,
                    "text/plain",
                    u32::from(*payload),
                    digest(*id),
                    handoff(*id, *artifact)?,
                    revision(false)?,
                )
                .map_err(debug)?,
            }
        }
        Operation::Acknowledge { id, correct_receiver } => Command::AcknowledgeMessage {
            message_id: message(*id)?,
            receiver: actor(
                model.messages.get(id).map_or(0, |row| row.receiver)
                    + if *correct_receiver { 0 } else { 8 },
            )?,
        },
        Operation::Complete { task: id, success, correct_owner, artifact } => {
            Command::CompleteTask {
                task_id: task(*id)?,
                completed_by: actor(model.acting(*id, *correct_owner))?,
                terminal: terminal(u8::from(!success), *id, *artifact)?,
            }
        }
        Operation::Abandon { task: id, correct_owner } => Command::AbandonTask {
            task_id: task(*id)?,
            abandoned_by: actor(model.acting(*id, *correct_owner))?,
            reason_digest: digest(*id),
        },
        Operation::Cancel { task: id, requester } => Command::CancelTask {
            task_id: task(*id)?,
            requested_by: actor(match requester {
                0 => model.owner(*id),
                1 => 0,
                _ => model.owner(*id) + 8,
            })?,
            reason_digest: digest(*id),
        },
        Operation::AcknowledgeCancellation { task: id, correct_owner } => {
            Command::AcknowledgeCancellation {
                task_id: task(*id)?,
                owner: actor(model.acting(*id, *correct_owner))?,
            }
        }
        Operation::Pause { paused: true, correct_owner } => {
            Command::Pause { requested_by: actor(if *correct_owner { 0 } else { 8 })? }
        }
        Operation::Pause { paused: false, correct_owner } => {
            Command::Resume { requested_by: actor(if *correct_owner { 0 } else { 8 })? }
        }
        Operation::Finalize => Command::Finalize,
        Operation::StaleFence => Command::Pause { requested_by: actor(0)? },
        Operation::ReplayLast | Operation::Reopen | Operation::Read => {
            return Err("non-command operation dispatched".into());
        }
    })
}
fn command(
    state: Option<&CollaborationState>,
    serial: u64,
    kind: Command,
    stale: bool,
) -> Result<CollaborationCommand, String> {
    CollaborationCommand::new(
        CommandId::new(bytes(serial)).map_err(debug)?,
        EventId::new(bytes(serial)).map_err(debug)?,
        RunId::new(bytes(0)).map_err(debug)?,
        state.map_or(0, |state| state.sequence().get()) + u64::from(stale),
        state.map(CollaborationState::last_event_id),
        state.map_or(Sha256Digest::new([0; 32]), CollaborationState::state_digest),
        revision(false)?,
        kind,
    )
    .map_err(debug)
}
fn phase(value: u8) -> TaskPhase {
    match value {
        0 => TaskPhase::Offered,
        1 => TaskPhase::Accepted,
        2 => TaskPhase::Active,
        3 => TaskPhase::Cancelling,
        _ => TaskPhase::Terminal,
    }
}
fn handoff_matches(actual: Option<ArtifactHandoff>, id: u8, kind: u8) -> Result<bool, String> {
    match (actual, kind) {
        (None, 0) => Ok(true),
        (Some(actual), 1 | 2) => Ok(actual.artifact_id()
            == ArtifactId::new(bytes(u64::from(id))).map_err(debug)?
            && actual.artifact_digest() == digest(id)
            && actual.evidence_digest() == digest(id.wrapping_add(1))
            && actual.revision() == revision(kind == 2)?),
        _ => Ok(false),
    }
}
fn terminal_matches(
    actual: Option<TaskTerminal>,
    expected: Option<(u8, u8, Option<u8>)>,
    id: u8,
) -> Result<bool, String> {
    match (actual, expected) {
        (None, None) => Ok(true),
        (Some(actual), Some((kind, artifact, cause))) => {
            let expected_kind = match kind {
                0 => TaskTerminalKind::Succeeded,
                1 => TaskTerminalKind::Failed,
                2 => TaskTerminalKind::Rejected,
                3 => TaskTerminalKind::Cancelled,
                _ => TaskTerminalKind::Abandoned,
            };
            let expected_cause = if kind == 3 {
                cause.map_or(Sha256Digest::new([0; 32]), digest)
            } else {
                digest(id)
            };
            Ok(actual.kind() == expected_kind
                && actual.cause_digest() == expected_cause
                && handoff_matches(actual.handoff(), id, artifact)?)
        }
        _ => Ok(false),
    }
}
fn compare(
    model: &Model,
    state: &CollaborationState,
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    let expected_phase = if model.terminal.is_some() {
        CollaborationPhase::Terminal
    } else if model.paused {
        CollaborationPhase::Paused
    } else {
        CollaborationPhase::Active
    };
    if state.sequence().get() != model.sequence
        || state.tasks().len() != model.tasks.len()
        || state.messages().len() != model.messages.len()
        || state.phase() != expected_phase
        || state.has_pending_directives() != model.pending()
    {
        return Err(
            "collaboration head/count/phase/pending state differs from independent history".into(),
        );
    }
    for (id, row) in &model.tasks {
        let actual = state.task(task(*id)?).ok_or("retained collaboration task missing")?;
        let reservation_matches = match (actual.reservation(), row.reservation) {
            (None, false) => true,
            (Some(actual), true) => {
                actual.work_id() == work(*id)?
                    && actual.dispatch_id()
                        == DispatchId::new(bytes(u64::from(*id))).map_err(debug)?
                    && actual.owner() == actor(row.owner)?
                    && actual.revision() == revision(false)?
            }
            _ => false,
        };
        let assignment = actual.assignment();
        if assignment.task_id() != task(*id)?
            || assignment.root_task_id() != task(0)?
            || assignment.parent_task_id() != row.parent.map(task).transpose()?
            || assignment.depth() != row.depth
            || assignment.owner() != actor(row.owner)?
            || assignment.parent_owner() != actor(model.parent_owner(*id))?
            || assignment.work_id() != work(*id)?
            || assignment.goal_digest() != digest(*id)
            || assignment.required() != row.required
            || assignment.join_policy() != join(row.join)
            || assignment.role()
                != if row.parent.is_none() { HarnessRole::Writer } else { HarnessRole::Reviewer }
            || actual.phase() != phase(row.phase)
            || !reservation_matches
            || !terminal_matches(actual.terminal(), row.end, *id)?
            || state.join_satisfied(task(*id)?) != model.joined(*id)
        {
            return Err(format!(
                "task {id} ownership/lifecycle/join diverged: {actual:?}; expected {row:?}"
            ));
        }
        let children = state
            .children(task(*id)?)
            .into_iter()
            .map(|child| child.assignment().task_id())
            .collect::<Vec<_>>();
        if children != model.children(*id).into_iter().map(task).collect::<Result<Vec<_>, _>>()? {
            return Err("parent map child projection differs".into());
        }
        let descendants = model.descendants(*id);
        for candidate in model.tasks.keys() {
            if state.is_descendant_of(task(*candidate)?, task(*id)?)
                != descendants.contains(candidate)
            {
                return Err(
                    "descendant membership differs from independent breadth-first closure".into()
                );
            }
        }
    }
    for (id, row) in &model.messages {
        let actual = state.message(message(*id)?).ok_or("causal message missing")?;
        let value = actual.message();
        if actual.acknowledged() != row.acknowledged
            || value.task_id() != task(row.task)?
            || value.root_task_id() != task(0)?
            || value.sender() != actor(row.sender)?
            || value.receiver() != actor(row.receiver)?
            || value.ordinal() != row.ordinal
            || value.predecessor() != row.predecessor.map(message).transpose()?
            || value.payload_bytes() != u32::from(row.payload)
            || value.media_type() != "text/plain"
            || value.content_digest() != digest(*id)
            || !handoff_matches(value.artifact(), *id, row.artifact)?
            || value.revision() != revision(false)?
        {
            return Err(format!(
                "causal message {id} content/predecessor/delivery binding diverged"
            ));
        }
    }
    match (model.terminal, state.terminal()) {
        (None, None) => {}
        (Some(kind), Some(actual)) => {
            let expected = match kind {
                0 => CollaborationTerminalKind::Completed,
                1 => CollaborationTerminalKind::Failed,
                2 => CollaborationTerminalKind::Cancelled,
                3 => CollaborationTerminalKind::Abandoned,
                _ => CollaborationTerminalKind::UnsatisfiedJoin,
            };
            let blocked = if kind == 0 {
                Vec::new()
            } else {
                model
                    .tasks
                    .iter()
                    .filter(|(_, row)| !matches!(row.end, Some((0, _, _))))
                    .map(|(id, _)| task(*id))
                    .collect::<Result<Vec<_>, _>>()?
            };
            if actual.kind() != expected || actual.blocking_tasks() != blocked {
                return Err("aggregate terminal truth or blocker set differs".into());
            }
        }
        _ => return Err("aggregate terminal was lost or fabricated".into()),
    }
    bump(metrics, "independent_state_comparisons");
    Ok(())
}
fn open(path: &std::path::Path) -> Result<SqliteJournal, String> {
    SqliteJournal::open(
        path,
        StoreId::new(bytes(0)).map_err(debug)?,
        SqliteJournalOptions::default(),
    )
    .map_err(debug)
}
pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    if case.root_join >= 3
        || case.depth == 0
        || case.depth > 4
        || case.fan_out == 0
        || case.fan_out > 4
        || case.recipients == 0
        || case.recipients > 2
        || case.artifacts == 0
        || case.artifacts > 4
        || case.operations.len() > 256
    {
        return Err("case outside collaboration pilot bounds".into());
    }
    let directory = tempfile::tempdir().map_err(debug)?;
    let path = directory.path().join("collaboration.sqlite");
    let mut journal = open(&path)?;
    let mut model = Model::new(case.root_join);
    let root = model.tasks.get(&0).ok_or("reference root missing")?;
    let binding = CollaborationBinding::new(
        CollaborationId::new(bytes(0)).map_err(debug)?,
        RunId::new(bytes(0)).map_err(debug)?,
        revision(false)?,
        SchedulerId::new(bytes(0)).map_err(debug)?,
        limits(case)?,
        assignment(&model, 0, root)?,
    )
    .map_err(debug)?;
    let request = command(None, 0, Command::Start { binding }, false)?;
    let transition = start(&request).map_err(debug)?;
    commit_collaboration_transition(&mut journal, &request, &transition).map_err(debug)?;
    let mut state = transition.into_state();
    let mut last = None;
    let mut metrics = crate::Metrics::new();
    let mut rejected = false;
    compare(&model, &state, &mut metrics)?;
    for (step, operation) in case.operations.iter().enumerate() {
        match operation {
            Operation::Reopen => {
                drop(journal);
                journal = open(&path)?;
                let restored = load_collaboration_replay(&journal, state.run_id())
                    .map_err(debug)?
                    .rebuild()
                    .map_err(debug)?
                    .ok_or("reopened collaboration absent")?;
                if restored != state {
                    return Err("SQLite reopen changed accepted collaboration".into());
                }
                bump(&mut metrics, "sqlite_reopens");
            }
            Operation::ReplayLast => {
                if let Some((request, transition)) = &last {
                    commit_collaboration_transition(&mut journal, request, transition)
                        .map_err(debug)?;
                    bump(&mut metrics, "exact_commit_replays");
                }
            }
            Operation::Read => bump(&mut metrics, "explicit_reads"),
            _ => {
                let mut next = model.clone();
                let mut next_metrics = metrics.clone();
                let expected = next.apply(case, operation, &mut next_metrics);
                let before = state.clone();
                let request = actual_kind(&model, operation).and_then(|kind| {
                    command(
                        Some(&state),
                        u64::try_from(step).map_err(debug)? + 1,
                        kind,
                        matches!(operation, Operation::StaleFence),
                    )
                });
                let actual = request.as_ref().map_err(Clone::clone).and_then(|request| {
                    decide(&state, request).map_err(|error| format!("typed:{:?}", error.kind()))
                });
                match (expected, actual) {
                    (Ok(()), Ok(transition)) => {
                        let request = request?;
                        commit_collaboration_transition(&mut journal, &request, &transition)
                            .map_err(debug)?;
                        state = transition.state().clone();
                        last = Some((request, transition));
                        model = next;
                        metrics = next_metrics;
                        bump(&mut metrics, "accepted_commands");
                        if rejected {
                            bump(&mut metrics, "accepted_after_rejection");
                            rejected = false;
                        }
                    }
                    (Err(expected), Err(actual)) => {
                        let constructor =
                            matches!(operation,Operation::Offer{task,parent,..}if task==parent);
                        if !constructor && actual != format!("typed:{expected:?}") {
                            return Err(format!(
                                "step {step} {operation:?}: expected {expected:?}, got {actual}"
                            ));
                        }
                        if state != before {
                            return Err(
                                "rejected collaboration command mutated accepted state".into()
                            );
                        }
                        rejected = true;
                        bump(&mut metrics, "rejected_commands");
                        if matches!(expected, Error::JoinUnsatisfied) {
                            bump(&mut metrics, "unsatisfied_join_rejections");
                        }
                        if matches!(expected, Error::LimitExceeded) {
                            bump(&mut metrics, "limit_rejections");
                        }
                        if matches!(expected, Error::OwnerMismatch) {
                            bump(&mut metrics, "owner_rejections");
                        }
                        if matches!(expected, Error::CausalityViolation) {
                            bump(&mut metrics, "causality_rejections");
                        }
                        if matches!(operation, Operation::Send { stale: true, .. })
                            && expected == Error::CausalityViolation
                        {
                            bump(&mut metrics, "stale_predecessor_rejections");
                        }
                    }
                    (expected, actual) => {
                        return Err(format!(
                            "step {step} {operation:?}: collaboration/model disagreement expected {expected:?}, actual {}",
                            actual.map(|_| "accepted".to_owned()).unwrap_or_else(|error| error)
                        ));
                    }
                }
            }
        }
        compare(&model, &state, &mut metrics)?;
        bump(&mut metrics, "operations");
    }
    let replay = load_collaboration_replay(&journal, state.run_id()).map_err(debug)?;
    if replay.events().len() != usize::try_from(model.sequence).map_err(debug)?
        || replay.rebuild().map_err(debug)?.as_ref() != Some(&state)
    {
        return Err("durable causal replay differs from independently accepted history".into());
    }
    bump(&mut metrics, "cases");
    Ok(metrics)
}
fn offer(id: u8, parent: u8, owner: u8, join: u8) -> Operation {
    Operation::Offer { task: id, parent, owner, required: true, join, correct_owner: true }
}
fn activate(id: u8) -> Operation {
    Operation::Activate { task: id, correct_binding: true }
}
fn accept(id: u8) -> Operation {
    Operation::Accept { task: id, correct_owner: true }
}
fn send(id: u8, task: u8, reverse: bool, stale: bool) -> Operation {
    Operation::Send { id, task, reverse, correct_sender: true, stale, payload: 7, artifact: 0 }
}
fn base(operations: Vec<Operation>) -> Case {
    Case { root_join: 1, depth: 4, fan_out: 4, recipients: 2, artifacts: 4, operations }
}
pub fn calibration() -> Case {
    base(vec![
        activate(0),
        offer(1, 0, 1, 1),
        accept(1),
        activate(1),
        offer(2, 1, 2, 0),
        accept(2),
        activate(2),
        Operation::Cancel { task: 1, requester: 1 },
        Operation::AcknowledgeCancellation { task: 1, correct_owner: true },
        Operation::AcknowledgeCancellation { task: 2, correct_owner: true },
        Operation::Complete { task: 0, success: true, correct_owner: true, artifact: 0 },
        Operation::Complete { task: 0, success: false, correct_owner: true, artifact: 0 },
        Operation::Finalize,
        Operation::Reopen,
    ])
}
fn causal_case(first: [u8; 3], second: [u8; 3], reverse: bool) -> Case {
    let mut operations = vec![activate(0), offer(1, 0, 1, 0), accept(1), activate(1)];
    for index in 0..3 {
        let root = first[index] + 10;
        let child = second[index] + 20;
        operations.extend([
            send(root, 0, false, false),
            Operation::Acknowledge { id: root, correct_receiver: true },
            send(child, 1, reverse, false),
            Operation::Reopen,
            Operation::Acknowledge { id: child, correct_receiver: true },
        ]);
    }
    operations.extend([
        send(60, 0, false, true),
        send(60, 0, false, false),
        Operation::Acknowledge { id: 60, correct_receiver: true },
        Operation::Complete { task: 0, success: true, correct_owner: true, artifact: 0 },
        Operation::Complete { task: 1, success: true, correct_owner: true, artifact: 1 },
        Operation::Complete { task: 0, success: true, correct_owner: true, artifact: 1 },
        Operation::ReplayLast,
        Operation::Finalize,
        Operation::Reopen,
    ]);
    base(operations)
}
pub fn examples() -> Vec<Case> {
    let mut cases = vec![calibration()];
    for first in [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]] {
        cases.push(causal_case(first, [2, 0, 1], true));
    }
    for root_join in 0..3 {
        let mut case = base(vec![
            activate(0),
            Operation::Complete { task: 0, success: true, correct_owner: true, artifact: 1 },
            Operation::Complete { task: 0, success: false, correct_owner: true, artifact: 0 },
            Operation::Finalize,
            Operation::Reopen,
        ]);
        case.root_join = root_join;
        cases.push(case);
    }
    let mut bounded = causal_case([0, 1, 2], [0, 1, 2], true);
    bounded.recipients = 1;
    bounded.artifacts = 1;
    bounded.operations.insert(5, send(61, 1, false, false));
    cases.push(bounded);
    let mut payloads = base(vec![activate(0)]);
    payloads.root_join = 0;
    payloads.artifacts = 1;
    let mut first = send(1, 0, false, false);
    if let Operation::Send { artifact, payload, .. } = &mut first {
        *artifact = 1;
        *payload = 1024;
    }
    let mut rejected = send(2, 0, false, false);
    if let Operation::Send { artifact, .. } = &mut rejected {
        *artifact = 1;
    }
    let mut oversized = send(2, 0, false, false);
    if let Operation::Send { payload, .. } = &mut oversized {
        *payload = 1025;
    }
    payloads.operations.extend([
        first,
        Operation::Acknowledge { id: 1, correct_receiver: true },
        rejected,
        oversized,
        send(2, 0, false, false),
        Operation::Acknowledge { id: 2, correct_receiver: true },
        Operation::Complete { task: 0, success: true, correct_owner: true, artifact: 1 },
        Operation::Complete { task: 0, success: true, correct_owner: true, artifact: 0 },
        Operation::Finalize,
        Operation::Reopen,
    ]);
    cases.push(payloads);
    cases
}

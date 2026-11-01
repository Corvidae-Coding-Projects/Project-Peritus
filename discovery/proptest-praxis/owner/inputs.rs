//! List-model investigation of immutable accepted inputs and host incorporation proposals.

use std::collections::BTreeMap;

use crate::control::{
    ControlError, ControlText, InputCapture, InputId, InputLedger, InputSelection, InputState,
    InvocationId, MAX_REQUEST_CONTEXT_BYTES, QueueIntent,
};
use peritus_types::ActorId;
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::json;

type Metrics = BTreeMap<String, u64>;
const MAX_FILE_CONTEXT_SUFFIX_BYTES: u16 = 16 * 1024;

/// Exact abstract history; identities and text resolve deterministically without production data.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Case {
    initial: Vec<Initial>,
    operations: Vec<Operation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Initial {
    text: String,
    dependencies: u8,
    held: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum Text {
    Literal(String),
    ByteBoundary { delta: i8, unicode: bool },
    Whitespace,
    TerminalControl,
}
impl Text {
    fn resolve(&self) -> Result<String, String> {
        Ok(match self {
            Self::Literal(text) => text.clone(),
            Self::Whitespace => " \t\n".into(),
            Self::TerminalControl => "inert\u{1b}[0m".into(),
            Self::ByteBoundary { delta, unicode } => {
                let length = usize::try_from(8192_i32 + i32::from(*delta))
                    .map_err(|error| error.to_string())?;
                if *unicode {
                    format!("{}{}", "é".repeat(length / 2), "x".repeat(length % 2))
                } else {
                    "x".repeat(length)
                }
            }
        })
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum Revision {
    Latest,
    First,
    Previous,
    Future,
    Zero,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
struct Target {
    id: u8,
    revision: Revision,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum Order {
    Reverse,
    Rotate(u8),
    Explicit(Vec<u8>),
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum Items {
    Current(u8),
    Saved(u8),
    Explicit(Vec<Target>),
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
enum Corruption {
    ZeroGeneration,
    ZeroRevision,
    ZeroAuthor,
    DuplicateOrder,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
enum Operation {
    Enqueue { id: u8, text: Text, dependencies: Vec<u8>, author: u8 },
    Edit { target: Target, text: Text, author: u8 },
    Hold { target: Target, held: bool },
    Withdraw(Target),
    Move { target: Target, before: Option<u8> },
    Reorder(Order),
    Correct { original: Target, id: u8, text: Text, author: u8 },
    Capture(u8),
    Incorporate { invocation: u8, items: Items, reverse: bool },
    Replies(Vec<(u8, String)>),
    Reopen,
    RejectImport(Corruption),
    FileContext { bytes: u16, unicode: bool },
    // Retain old serialized witnesses, but fail explicitly before their multi-GiB allocation.
    FileContextBoundary(i8),
}

fn target_strategy() -> BoxedStrategy<Target> {
    (
        0_u8..12,
        proptest::sample::select(vec![
            Revision::Latest,
            Revision::Latest,
            Revision::First,
            Revision::Previous,
            Revision::Future,
            Revision::Zero,
        ]),
    )
        .prop_map(|(id, revision)| Target { id, revision })
        .boxed()
}
fn literal_strategy() -> BoxedStrategy<String> {
    proptest::collection::vec(
        proptest::sample::select(vec![
            'a', 'b', 'é', '中', '🙂', '\u{301}', ' ', '\n', '\t', '"', '\\',
        ]),
        0..=24,
    )
    .prop_map(|characters| format!("input:{}", characters.into_iter().collect::<String>()))
    .boxed()
}
fn text_strategy() -> BoxedStrategy<Text> {
    prop_oneof![12 => literal_strategy().prop_map(Text::Literal),
        1 => Just(Text::Whitespace), 1 => Just(Text::TerminalControl)]
    .boxed()
}

/// Histories use a small ID space, explicit snapshots, and intentional near-valid operations.
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let items = prop_oneof![
        (0_u8..=12).prop_map(Items::Current),
        (0_u8..4).prop_map(Items::Saved),
        proptest::collection::vec(target_strategy(), 0..=6).prop_map(Items::Explicit)
    ];
    let order = prop_oneof![
        Just(Order::Reverse),
        any::<u8>().prop_map(Order::Rotate),
        proptest::collection::vec(0_u8..12, 0..=8).prop_map(Order::Explicit)
    ];
    let operation = prop_oneof![
        5 => (0_u8..12, text_strategy(), proptest::collection::vec(0_u8..12, 0..=3), 0_u8..3)
            .prop_map(|(id, text, dependencies, author)| Operation::Enqueue { id, text, dependencies, author }),
        4 => (target_strategy(), text_strategy(), 0_u8..3).prop_map(|(target, text, author)| Operation::Edit { target, text, author }),
        4 => (target_strategy(), any::<bool>()).prop_map(|(target, held)| Operation::Hold { target, held }),
        3 => target_strategy().prop_map(Operation::Withdraw),
        3 => (target_strategy(), proptest::option::of(0_u8..12)).prop_map(|(target, before)| Operation::Move { target, before }),
        3 => order.prop_map(Operation::Reorder),
        3 => (target_strategy(), 0_u8..12, text_strategy(), 0_u8..3).prop_map(|(original, id, text, author)| Operation::Correct { original, id, text, author }),
        4 => (0_u8..4).prop_map(Operation::Capture),
        6 => (0_u8..8, items, any::<bool>()).prop_map(|(invocation, items, reverse)| Operation::Incorporate { invocation, items, reverse }),
        2 => proptest::collection::vec((0_u8..8, literal_strategy()), 0..=4).prop_map(Operation::Replies),
        2 => Just(Operation::Reopen),
        2 => (0_u16..=MAX_FILE_CONTEXT_SUFFIX_BYTES, any::<bool>())
            .prop_map(|(bytes, unicode)| Operation::FileContext { bytes, unicode }),
        1 => proptest::sample::select(vec![Corruption::ZeroGeneration, Corruption::ZeroRevision,
            Corruption::ZeroAuthor, Corruption::DuplicateOrder]).prop_map(Operation::RejectImport),
    ];
    (
        proptest::collection::vec(
            (literal_strategy(), any::<u8>(), any::<bool>())
                .prop_map(|(text, dependencies, held)| Initial { text, dependencies, held }),
            1..=6,
        ),
        proptest::collection::vec(operation, 0..=max_steps.min(128)),
    )
        .prop_map(|(initial, operations)| Case { initial, operations })
        .boxed()
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Selection {
    id: u8,
    revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum State {
    Queued,
    Held,
    Incorporated,
    Superseded,
    Withdrawn,
}
impl State {
    fn production(self) -> InputState {
        match self {
            Self::Queued => InputState::Queued,
            Self::Held => InputState::Held,
            Self::Incorporated => InputState::Incorporated,
            Self::Superseded => InputState::Superseded,
            Self::Withdrawn => InputState::Withdrawn,
        }
    }
    fn pending(self) -> bool {
        matches!(self, Self::Queued | Self::Held)
    }
}

#[derive(Clone, Debug)]
struct Row {
    selected: Selection,
    author: u8,
    text: String,
    dependencies: Vec<u8>,
    state: State,
    correction: Option<Selection>,
    invocation: Option<u8>,
}
#[derive(Clone, Debug)]
struct Request {
    invocation: u8,
    semantic_digest: [u8; 32],
    manifest: [u8; 32],
    items: Vec<Selection>,
}
#[derive(Clone, Debug, Default)]
struct Model {
    rows: Vec<Row>,
    order: Vec<u8>,
    requests: Vec<Request>,
    generation: u64,
}
#[derive(Clone, Debug)]
struct Capture {
    generation: u64,
    pending: Vec<Selection>,
    included: Vec<Selection>,
    replies: Vec<u8>,
    conversation: String,
}

#[derive(Clone, Debug)]
enum Command {
    Enqueue { id: u8, text: String, dependencies: Vec<u8> },
    Edit { selected: Selection, text: String },
    Hold { selected: Selection, held: bool },
    Withdraw(Selection),
    Move { selected: Selection, before: Option<u8> },
    Reorder(Vec<u8>),
    Correct { original: Selection, id: u8, text: String },
    Incorporate(Request),
}

fn bytes(id: u8) -> [u8; 16] {
    (u128::from(id) + 1).to_be_bytes()
}
fn input_id(id: u8) -> Result<InputId, ControlError> {
    InputId::new(bytes(id))
}
fn invocation_id(id: u8) -> Result<InvocationId, ControlError> {
    InvocationId::new(bytes(id))
}
fn selection(value: Selection) -> Result<InputSelection, ControlError> {
    InputSelection::new(input_id(value.id)?, value.revision)
}
fn selections(values: &[Selection]) -> Result<Vec<InputSelection>, ControlError> {
    values.iter().copied().map(selection).collect()
}
fn bump(metrics: &mut Metrics, name: &str, amount: u64) {
    *metrics.entry(name.into()).or_default() += amount;
}

impl Command {
    fn intent(&self) -> Result<QueueIntent, ControlError> {
        Ok(match self {
            Self::Enqueue { id, text, dependencies } => QueueIntent::Enqueue {
                id: input_id(*id)?,
                text: ControlText::new(text.clone())?,
                dependencies: dependencies
                    .iter()
                    .copied()
                    .map(input_id)
                    .collect::<Result<_, _>>()?,
            },
            Self::Edit { selected, text } => QueueIntent::Edit {
                selected: selection(*selected)?,
                text: ControlText::new(text.clone())?,
            },
            Self::Hold { selected, held } => {
                QueueIntent::Hold { selected: selection(*selected)?, held: *held }
            }
            Self::Withdraw(selected) => QueueIntent::Withdraw(selection(*selected)?),
            Self::Move { selected, before } => QueueIntent::Move {
                selected: selection(*selected)?,
                before: before.map(input_id).transpose()?,
            },
            Self::Reorder(order) => {
                QueueIntent::Reorder(order.iter().copied().map(input_id).collect::<Result<_, _>>()?)
            }
            Self::Correct { original, id, text } => QueueIntent::Correct {
                original: selection(*original)?,
                id: input_id(*id)?,
                text: ControlText::new(text.clone())?,
            },
            Self::Incorporate(request) => QueueIntent::Incorporate {
                invocation: invocation_id(request.invocation)?,
                request_digest: request.semantic_digest,
                manifest_digest: request.manifest,
                items: selections(&request.items)?,
            },
        })
    }
    fn constructor_valid(&self) -> bool {
        let nonzero = |selected: Selection| selected.revision != 0;
        match self {
            Self::Enqueue { text, .. } => valid_text(text),
            Self::Edit { selected, text } | Self::Correct { original: selected, text, .. } => {
                nonzero(*selected) && valid_text(text)
            }
            Self::Hold { selected, .. }
            | Self::Withdraw(selected)
            | Self::Move { selected, .. } => nonzero(*selected),
            Self::Incorporate(request) => request.items.iter().copied().all(nonzero),
            Self::Reorder(_) => true,
        }
    }
}
fn valid_text(text: &str) -> bool {
    text.len() <= 8192
        && !text.trim().is_empty()
        && text
            .chars()
            .all(|character| !character.is_control() || ['\n', '\t'].contains(&character))
}

impl Model {
    fn latest(&self, id: u8) -> Option<&Row> {
        self.rows.iter().rev().find(|row| row.selected.id == id)
    }
    fn exact(&self, selected: Selection) -> Result<&Row, ControlError> {
        self.rows.iter().find(|row| row.selected == selected).ok_or(ControlError::StaleRevision)
    }
    fn pending_index(&self, selected: Selection) -> Result<usize, ControlError> {
        let index = self
            .rows
            .iter()
            .rposition(|row| row.selected.id == selected.id)
            .ok_or(ControlError::NotFound)?;
        let row = &self.rows[index];
        if row.selected != selected {
            return Err(ControlError::StaleRevision);
        }
        if !row.state.pending() {
            return Err(ControlError::InvalidInput);
        }
        Ok(index)
    }
    fn resolve(&self, target: Target) -> Selection {
        let latest = self.latest(target.id).map_or(1, |row| row.selected.revision);
        Selection {
            id: target.id,
            revision: match target.revision {
                Revision::Latest => latest,
                Revision::First => 1,
                Revision::Previous => latest.saturating_sub(1).max(1),
                Revision::Future => latest.saturating_add(1),
                Revision::Zero => 0,
            },
        }
    }
    fn enqueue(
        &mut self,
        id: u8,
        author: u8,
        text: String,
        dependencies: Vec<u8>,
        correction: Option<Selection>,
    ) -> Result<(), ControlError> {
        if self.latest(id).is_some() {
            return Err(ControlError::IdempotencyConflict);
        }
        if dependencies.iter().enumerate().any(|(index, dependency)| {
            *dependency == id
                || self.latest(*dependency).is_none()
                || dependencies[..index].contains(dependency)
        }) {
            return Err(ControlError::InvalidInput);
        }
        self.rows.push(Row {
            selected: Selection { id, revision: 1 },
            author,
            text,
            dependencies,
            state: State::Queued,
            correction,
            invocation: None,
        });
        self.order.push(id);
        Ok(())
    }
    fn apply(&self, command: &Command, author: u8) -> Result<Self, ControlError> {
        if !command.constructor_valid() {
            return Err(ControlError::InvalidInput);
        }
        let mut next = self.clone();
        next.mutate(command, author)?;
        if !matches!(command, Command::Incorporate(_)) {
            next.generation += 1;
        }
        if !next.order_valid()
            || (next.generation == 0 && (!next.rows.is_empty() || !next.requests.is_empty()))
        {
            return Err(ControlError::InvalidInput);
        }
        Ok(next)
    }
    fn mutate(&mut self, command: &Command, author: u8) -> Result<(), ControlError> {
        match command {
            Command::Enqueue { id, text, dependencies } => {
                self.enqueue(*id, author, text.clone(), dependencies.clone(), None)
            }
            Command::Edit { selected, text } => {
                let index = self.pending_index(*selected)?;
                let mut row = self.rows[index].clone();
                row.selected.revision += 1;
                row.author = author;
                row.text.clone_from(text);
                self.rows[index].state = State::Superseded;
                self.rows.push(row);
                Ok(())
            }
            Command::Hold { selected, held } => {
                let index = self.pending_index(*selected)?;
                self.rows[index].state = if *held { State::Held } else { State::Queued };
                Ok(())
            }
            Command::Withdraw(selected) => {
                if self.order.iter().any(|id| {
                    self.latest(*id).is_some_and(|row| row.dependencies.contains(&selected.id))
                }) {
                    return Err(ControlError::InvalidInput);
                }
                let index = self.pending_index(*selected)?;
                self.rows[index].state = State::Withdrawn;
                self.order.retain(|id| *id != selected.id);
                Ok(())
            }
            Command::Move { selected, before } => {
                self.pending_index(*selected)?;
                if *before == Some(selected.id) {
                    return Err(ControlError::InvalidInput);
                }
                self.order.retain(|id| *id != selected.id);
                let destination = before.map_or(Ok(self.order.len()), |before| {
                    self.order.iter().position(|id| *id == before).ok_or(ControlError::NotFound)
                })?;
                self.order.insert(destination, selected.id);
                Ok(())
            }
            Command::Reorder(order) => {
                // Existing pending identities are unique, so equal length and membership also
                // require exactly one occurrence of each identity in the proposed order.
                if order.len() != self.order.len()
                    || self.order.iter().any(|id| !order.contains(id))
                {
                    return Err(ControlError::InvalidInput);
                }
                self.order.clone_from(order);
                Ok(())
            }
            Command::Correct { original, id, text } => {
                if self.exact(*original)?.state != State::Incorporated {
                    return Err(ControlError::InvalidInput);
                }
                self.enqueue(*id, author, text.clone(), vec![original.id], Some(*original))
            }
            Command::Incorporate(request) => self.incorporate(request),
        }
    }
    fn incorporate(&mut self, request: &Request) -> Result<(), ControlError> {
        if self.requests.iter().any(|previous| previous.invocation == request.invocation) {
            return Err(ControlError::IdempotencyConflict);
        }
        let selected_ids = request.items.iter().map(|item| item.id).collect::<Vec<_>>();
        if self.order.iter().filter(|id| selected_ids.contains(id)).copied().collect::<Vec<_>>()
            != selected_ids
        {
            return Err(ControlError::InvalidInput);
        }
        let mut included = self
            .requests
            .iter()
            .flat_map(|prior| prior.items.iter().map(|item| item.id))
            .collect::<Vec<_>>();
        let indices = request
            .items
            .iter()
            .map(|selected| {
                let index = self.pending_index(*selected)?;
                if self.rows[index].state != State::Queued {
                    return Err(ControlError::InvalidInput);
                }
                Ok(index)
            })
            .collect::<Result<Vec<_>, _>>()?;
        for (selected, index) in request.items.iter().zip(indices) {
            if self.rows[index].dependencies.iter().any(|id| !included.contains(id)) {
                return Err(ControlError::InvalidInput);
            }
            self.rows[index].state = State::Incorporated;
            self.rows[index].invocation = Some(request.invocation);
            included.push(selected.id);
        }
        self.order.retain(|id| !selected_ids.contains(id));
        self.requests.push(request.clone());
        Ok(())
    }
    fn order_valid(&self) -> bool {
        self.order.iter().enumerate().all(|(index, id)| {
            self.latest(*id).is_some_and(|row| {
                row.dependencies.iter().all(|dependency| {
                    self.latest(*dependency).is_some_and(|prior| {
                        prior.state == State::Incorporated
                            || self.order[..index].contains(dependency)
                    })
                })
            })
        })
    }
    fn capture(&self, pending: bool) -> Result<Capture, ControlError> {
        let mut included =
            self.requests.iter().flat_map(|request| request.items.clone()).collect::<Vec<_>>();
        let mut pending_items = Vec::new();
        if pending {
            for id in &self.order {
                let row = self.latest(*id).ok_or(ControlError::InvalidInput)?;
                if row.state == State::Queued
                    && row.dependencies.iter().all(|dependency| {
                        included.iter().any(|selected| selected.id == *dependency)
                    })
                {
                    pending_items.push(row.selected);
                    included.push(row.selected);
                }
            }
        }
        let conversation = included
            .iter()
            .map(|selected| self.exact(*selected).map(|row| format!("User: {}", row.text)))
            .collect::<Result<Vec<_>, _>>()?
            .join("\n\n");
        if conversation.len() > MAX_REQUEST_CONTEXT_BYTES {
            return Err(ControlError::Capacity);
        }
        Ok(Capture {
            generation: self.generation,
            pending: pending_items,
            included,
            replies: Vec::new(),
            conversation,
        })
    }
    fn capture_replies(
        &self,
        replies: &BTreeMap<u8, String>,
        pending: bool,
    ) -> Result<Capture, ControlError> {
        let mut capture = self.capture(pending)?;
        if replies.values().map(|text| text.len() + 32).sum::<usize>() + capture.conversation.len()
            > MAX_REQUEST_CONTEXT_BYTES
        {
            return Err(ControlError::Capacity);
        }
        if replies.keys().any(|id| !self.requests.iter().any(|request| request.invocation == *id)) {
            return Err(ControlError::InvalidInput);
        }
        let last_user =
            self.requests.iter().rposition(|request| !request.items.is_empty()).unwrap_or(0);
        let mut parts = Vec::new();
        for (index, request) in self.requests.iter().enumerate() {
            parts.extend(
                request
                    .items
                    .iter()
                    .map(|selected| self.exact(*selected).map(|row| format!("User: {}", row.text)))
                    .collect::<Result<Vec<_>, _>>()?,
            );
            if (!capture.pending.is_empty() || index < last_user)
                && let Some(reply) = replies.get(&request.invocation)
            {
                parts.push(format!("Peritus (public reply): {reply}"));
                capture.replies.push(request.invocation);
            }
        }
        parts.extend(
            capture
                .pending
                .iter()
                .map(|selected| self.exact(*selected).map(|row| format!("User: {}", row.text)))
                .collect::<Result<Vec<_>, _>>()?,
        );
        // The public reply delimiter is prepended even when an empty invocation precedes the
        // first input. Preserve that exact public representation without inventing a user turn.
        capture.conversation = parts.join("\n\n");
        if parts.first().is_some_and(|text| text.starts_with("Peritus (public reply):")) {
            capture.conversation.insert_str(0, "\n\n");
        }
        Ok(capture)
    }
}

struct Harness {
    actual: InputLedger,
    model: Model,
    saved: [Capture; 4],
    replies: BTreeMap<u8, String>,
    metrics: Metrics,
}
impl Harness {
    fn new() -> Result<Self, String> {
        let model = Model::default();
        let capture = model.capture(true).map_err(|error| error.to_string())?;
        Ok(Self {
            actual: InputLedger::default(),
            model,
            saved: std::array::from_fn(|_| capture.clone()),
            replies: BTreeMap::new(),
            metrics: Metrics::new(),
        })
    }
    fn apply(&mut self, command: &Command, author: u8) -> Result<(), String> {
        let expected = self.model.apply(command, author);
        let actor = ActorId::new(bytes(author)).map_err(|error| format!("{error:?}"))?;
        let before = self.actual.clone();
        let actual = command.intent().and_then(|intent| self.actual.apply(actor, &intent));
        match (actual, expected) {
            (Ok(next), Ok(model)) => {
                self.actual = next;
                self.model = model;
                bump(&mut self.metrics, "accepted_transitions", 1);
                bump(&mut self.metrics, command_name(command), 1);
            }
            (Err(actual), Err(expected)) if actual == expected => {
                bump(&mut self.metrics, "rejected_transitions", 1);
                bump(&mut self.metrics, &format!("rejected_{expected:?}"), 1);
                bump(&mut self.metrics, "continued_after_rejection", 1);
            }
            (actual, expected) => {
                return Err(format!(
                    "input transition mismatch for {command:?}: actual={actual:?}, model={expected:?}"
                ));
            }
        }
        if before.generation() != self.actual.generation()
            && matches!(command, Command::Incorporate(_))
        {
            return Err("incorporation incorrectly invalidated its own input effect fence".into());
        }
        self.verify()
    }
    fn verify(&mut self) -> Result<(), String> {
        verify_rows(&self.actual, &self.model)?;
        let before = self.actual.clone();
        compare_capture(self.actual.capture(), self.model.capture(true), &mut self.metrics)?;
        let actual = self.actual.incorporated_conversation();
        let expected = self.model.capture(false).map(|capture| capture.conversation);
        if actual != expected {
            return Err(format!("incorporated-only view mismatch: {actual:?} != {expected:?}"));
        }
        let replies = self
            .replies
            .iter()
            .map(|(id, text)| invocation_id(*id).map(|id| (id, text.clone())))
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map_err(|error| error.to_string())?;
        for pending in [false, true] {
            compare_capture(
                self.actual.capture_with_replies(&replies, pending),
                self.model.capture_replies(&self.replies, pending),
                &mut self.metrics,
            )?;
        }
        if before != self.actual {
            return Err("capturing inputs mutated accepted state".into());
        }
        bump(&mut self.metrics, "state_observations", 1);
        Ok(())
    }
    fn command(&self, op: &Operation, serial: usize) -> Result<Option<(Command, u8)>, String> {
        let command = match op {
            Operation::Enqueue { id, text, dependencies, author } => (
                Command::Enqueue {
                    id: *id,
                    text: text.resolve()?,
                    dependencies: dependencies.clone(),
                },
                *author,
            ),
            Operation::Edit { target, text, author } => (
                Command::Edit { selected: self.model.resolve(*target), text: text.resolve()? },
                *author,
            ),
            Operation::Hold { target, held } => {
                (Command::Hold { selected: self.model.resolve(*target), held: *held }, 0)
            }
            Operation::Withdraw(target) => (Command::Withdraw(self.model.resolve(*target)), 0),
            Operation::Move { target, before } => {
                (Command::Move { selected: self.model.resolve(*target), before: *before }, 0)
            }
            Operation::Correct { original, id, text, author } => (
                Command::Correct {
                    original: self.model.resolve(*original),
                    id: *id,
                    text: text.resolve()?,
                },
                *author,
            ),
            Operation::Reorder(order) => (Command::Reorder(self.order(order)), 0),
            Operation::Incorporate { invocation, items, reverse } => {
                let mut items = self.items(items)?;
                if *reverse {
                    items.reverse();
                }
                let mut request = [0; 32];
                request[..8].copy_from_slice(
                    &u64::try_from(serial).map_err(|error| error.to_string())?.to_be_bytes(),
                );
                let mut manifest = request;
                manifest[31] = 23;
                (
                    Command::Incorporate(Request {
                        invocation: *invocation,
                        semantic_digest: request,
                        manifest,
                        items,
                    }),
                    0,
                )
            }
            _ => return Ok(None),
        };
        Ok(Some(command))
    }
    fn order(&self, order: &Order) -> Vec<u8> {
        let mut values = self.model.order.clone();
        match order {
            Order::Reverse => values.reverse(),
            Order::Rotate(amount) if !values.is_empty() => {
                let amount = usize::from(*amount) % values.len();
                values.rotate_left(amount);
            }
            Order::Explicit(order) => values.clone_from(order),
            Order::Rotate(_) => {}
        }
        values
    }
    fn items(&self, items: &Items) -> Result<Vec<Selection>, String> {
        Ok(match items {
            Items::Current(count) => self
                .model
                .capture(true)
                .map_err(|error| error.to_string())?
                .pending
                .into_iter()
                .take(usize::from(*count))
                .collect(),
            Items::Saved(slot) => self.saved[usize::from(*slot) % self.saved.len()].pending.clone(),
            Items::Explicit(targets) => {
                targets.iter().map(|target| self.model.resolve(*target)).collect()
            }
        })
    }
    fn auxiliary(&mut self, operation: &Operation) -> Result<(), String> {
        match operation {
            Operation::Capture(slot) => {
                let expected = self.model.capture(true).map_err(|error| error.to_string())?;
                self.saved[usize::from(*slot) % 4] = expected;
                bump(&mut self.metrics, "saved_captures", 1);
            }
            Operation::Replies(values) => {
                self.replies = values.iter().cloned().collect();
                bump(&mut self.metrics, "reply_map_changes", 1);
            }
            Operation::Reopen => {
                let encoded =
                    serde_json::to_vec(&self.actual).map_err(|error| error.to_string())?;
                let reopened: InputLedger =
                    serde_json::from_slice(&encoded).map_err(|error| error.to_string())?;
                if reopened != self.actual {
                    return Err("serde reconstruction changed ledger fields".into());
                }
                self.actual = reopened;
                bump(&mut self.metrics, "serde_reconstructions", 1);
            }
            Operation::RejectImport(corruption) => self.reject_import(*corruption)?,
            Operation::FileContext { bytes, unicode } => self.file_context(*bytes, *unicode)?,
            Operation::FileContextBoundary(_) => {
                return Err(
                    "setup-incomplete: actual aggregate representation ceiling exceeds bounded local campaign"
                        .into(),
                );
            }
            _ => return Err("unknown non-transition operation".into()),
        }
        self.verify()
    }
    fn reject_import(&mut self, corruption: Corruption) -> Result<(), String> {
        let mut value = serde_json::to_value(&self.actual).map_err(|error| error.to_string())?;
        match corruption {
            Corruption::ZeroGeneration => value["generation"] = json!(0),
            Corruption::ZeroRevision => value["revisions"][0]["selection"]["revision"] = json!(0),
            Corruption::ZeroAuthor => value["revisions"][0]["author"] = json!(vec![0_u8; 16]),
            Corruption::DuplicateOrder => {
                let id = value["revisions"][0]["selection"]["id"].clone();
                value["order"].as_array_mut().ok_or("order is not array")?.extend([id.clone(), id]);
            }
        }
        let imported: InputLedger =
            serde_json::from_value(value).map_err(|error| error.to_string())?;
        if imported.capture() != Err(ControlError::InvalidInput) {
            return Err(format!(
                "invalid imported state {corruption:?} was not rejected at capture"
            ));
        }
        bump(&mut self.metrics, &format!("invalid_import_{corruption:?}"), 1);
        Ok(())
    }
    fn file_context(&mut self, bytes: u16, unicode: bool) -> Result<(), String> {
        if bytes > MAX_FILE_CONTEXT_SUFFIX_BYTES {
            return Err(format!(
                "setup-incomplete: file-context suffix exceeds bounded local allocation: {bytes} > {MAX_FILE_CONTEXT_SUFFIX_BYTES} bytes"
            ));
        }
        let captured = self.actual.capture().map_err(|error| error.to_string())?;
        let mut expected = self.model.capture(true).map_err(|error| error.to_string())?;
        let size = usize::from(bytes);
        let suffix = if unicode {
            format!("{}{}", "é".repeat(size / 2), "x".repeat(size % 2))
        } else {
            "x".repeat(size)
        };
        expected.conversation.push_str(&suffix);
        let result = captured.with_file_context(&suffix);
        compare_capture(result, Ok(expected), &mut self.metrics)?;
        bump(&mut self.metrics, "bounded_file_context_appends", 1);
        bump(&mut self.metrics, "bounded_file_context_bytes", u64::from(bytes));
        let maximum = self.metrics.entry("max_bounded_file_context_bytes".into()).or_default();
        *maximum = (*maximum).max(u64::from(bytes));
        if bytes == 0 {
            bump(&mut self.metrics, "empty_file_contexts", 1);
        }
        if !suffix.is_ascii() {
            bump(&mut self.metrics, "utf8_file_contexts", 1);
        }
        Ok(())
    }
}

fn command_name(command: &Command) -> &'static str {
    match command {
        Command::Enqueue { .. } => "enqueues",
        Command::Edit { .. } => "edits",
        Command::Hold { held: true, .. } => "holds",
        Command::Hold { held: false, .. } => "unholds",
        Command::Withdraw(_) => "withdrawals",
        Command::Move { .. } => "moves",
        Command::Reorder(_) => "reorders",
        Command::Correct { .. } => "corrections",
        Command::Incorporate(_) => "incorporations",
    }
}
fn verify_rows(actual: &InputLedger, model: &Model) -> Result<(), String> {
    if actual.generation() != model.generation
        || actual.revisions().len() != model.rows.len()
        || actual.order()
            != model
                .order
                .iter()
                .copied()
                .map(input_id)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| error.to_string())?
        || actual.invocations().len() != model.requests.len()
    {
        return Err(
            "ledger generation/history/pending order/request count differs from list model".into(),
        );
    }
    for (actual, expected) in actual.revisions().iter().zip(&model.rows) {
        if actual.selection() != selection(expected.selected).map_err(|error| error.to_string())?
            || actual.author_bytes() != &bytes(expected.author)
            || actual.text() != expected.text
            || actual.state() != expected.state.production()
            || actual.dependencies()
                != expected
                    .dependencies
                    .iter()
                    .copied()
                    .map(input_id)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(|error| error.to_string())?
            || actual.correction_of()
                != expected
                    .correction
                    .map(selection)
                    .transpose()
                    .map_err(|error| error.to_string())?
            || actual.invocation()
                != expected
                    .invocation
                    .map(invocation_id)
                    .transpose()
                    .map_err(|error| error.to_string())?
        {
            return Err(format!(
                "immutable input row changed: expected {expected:?}; got {actual:?}"
            ));
        }
    }
    for (actual, expected) in actual.invocations().iter().zip(&model.requests) {
        if actual.invocation()
            != invocation_id(expected.invocation).map_err(|error| error.to_string())?
            || actual.request_digest().into_bytes() != expected.semantic_digest
            || actual.manifest_digest().into_bytes() != expected.manifest
            || actual.items() != selections(&expected.items).map_err(|error| error.to_string())?
        {
            return Err("exact invocation/source-manifest/request binding changed".into());
        }
    }
    let empty = model.rows.is_empty()
        && model.order.is_empty()
        && model.requests.is_empty()
        && model.generation == 0;
    if actual.is_empty() != empty {
        return Err("empty-ledger classification changed".into());
    }
    Ok(())
}
fn compare_capture(
    actual: Result<InputCapture, ControlError>,
    expected: Result<Capture, ControlError>,
    metrics: &mut Metrics,
) -> Result<(), String> {
    match (actual, expected) {
        (Ok(actual), Ok(expected)) => {
            if actual.generation() != expected.generation
                || actual.conversation() != expected.conversation
                || actual.pending()
                    != selections(&expected.pending).map_err(|error| error.to_string())?
                || actual.included()
                    != selections(&expected.included).map_err(|error| error.to_string())?
                || actual.public_replies()
                    != expected
                        .replies
                        .iter()
                        .copied()
                        .map(invocation_id)
                        .collect::<Result<Vec<_>, _>>()
                        .map_err(|error| error.to_string())?
            {
                return Err(format!(
                    "capture differs from independent eligible-input/reply model: {actual:?}; expected {expected:?}"
                ));
            }
            bump(metrics, "captures", 1);
            bump(
                metrics,
                "captured_pending_inputs",
                u64::try_from(expected.pending.len()).map_err(|error| error.to_string())?,
            );
            bump(
                metrics,
                "captured_public_replies",
                u64::try_from(expected.replies.len()).map_err(|error| error.to_string())?,
            );
        }
        (Err(actual), Err(expected)) if actual == expected => {
            bump(metrics, "rejected_capture_queries", 1);
        }
        (actual, expected) => {
            return Err(format!("capture outcome mismatch: {actual:?}; expected {expected:?}"));
        }
    }
    Ok(())
}

/// Exercises actual public ledger transitions and checked capture after every success or failure.
pub fn check(case: &Case) -> Result<Metrics, String> {
    if case.initial.len() > 8 || case.operations.len() > 128 {
        return Err("case exceeds input-ledger pilot bounds".into());
    }
    let mut harness = Harness::new()?;
    bump(&mut harness.metrics, "cases", 1);
    for (index, initial) in case.initial.iter().enumerate() {
        let id = u8::try_from(index).map_err(|error| error.to_string())?;
        let dependencies = (0..index)
            .filter(|prior| initial.dependencies & (1 << prior) != 0)
            .map(u8::try_from)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        harness.apply(&Command::Enqueue { id, text: initial.text.clone(), dependencies }, 0)?;
        if initial.held {
            harness
                .apply(&Command::Hold { selected: Selection { id, revision: 1 }, held: true }, 0)?;
        }
    }
    let initial_capture = harness.model.capture(true).map_err(|error| error.to_string())?;
    harness.saved = std::array::from_fn(|_| initial_capture.clone());
    for (index, operation) in case.operations.iter().enumerate() {
        let result = if let Some((command, author)) = harness.command(operation, index + 1)? {
            if let Command::Incorporate(request) = &command {
                bump(&mut harness.metrics, "incorporation_attempts", 1);
                if request.items.is_empty() {
                    bump(&mut harness.metrics, "empty_incorporation_attempts", 1);
                }
            }
            harness.apply(&command, author)
        } else {
            harness.auxiliary(operation)
        };
        result.map_err(|error| format!("{error}; operation {index}, {operation:?}"))?;
        bump(&mut harness.metrics, "operations", 1);
    }
    harness.verify()?;
    Ok(harness.metrics)
}

fn initial(text: &str, dependencies: u8, held: bool) -> Initial {
    Initial { text: text.into(), dependencies, held }
}
fn latest(id: u8) -> Target {
    Target { id, revision: Revision::Latest }
}
fn incorporate(invocation: u8, count: u8) -> Operation {
    Operation::Incorporate { invocation, items: Items::Current(count), reverse: false }
}

/// Incorporation records exact inputs without changing the governing generation fence.
pub fn calibration_case() -> Case {
    Case {
        initial: vec![initial("exact governing input", 0, false)],
        operations: vec![incorporate(0, 255), Operation::Reopen],
    }
}

fn transition_examples() -> Vec<Case> {
    vec![
        calibration_case(),
        Case {
            initial: vec![],
            operations: vec![
                incorporate(0, 255),
                Operation::Reorder(Order::Reverse),
                Operation::Reopen,
            ],
        },
        Case {
            initial: vec![
                initial("held prerequisite", 0, true),
                initial("dependent", 1, false),
                initial("independent", 0, false),
            ],
            operations: vec![
                Operation::Capture(0),
                incorporate(0, 255),
                Operation::Hold { target: latest(0), held: false },
                Operation::Incorporate { invocation: 1, items: Items::Saved(0), reverse: false },
                incorporate(2, 255),
                Operation::Correct {
                    original: latest(0),
                    id: 5,
                    text: Text::Literal("new explicit correction".into()),
                    author: 2,
                },
                Operation::Edit {
                    target: latest(0),
                    text: Text::Literal("cannot rewrite incorporated".into()),
                    author: 1,
                },
                Operation::Replies(vec![
                    (0, "first public reply".into()),
                    (2, "latest public reply".into()),
                ]),
                incorporate(3, 255),
                Operation::Reopen,
            ],
        },
        Case {
            initial: vec![
                initial("first", 0, false),
                initial("second depends on first", 1, false),
                initial("unrelated", 0, false),
            ],
            operations: vec![
                Operation::Capture(1),
                Operation::Edit {
                    target: latest(0),
                    text: Text::Literal("FIRST".into()),
                    author: 1,
                },
                Operation::Edit {
                    target: Target { id: 0, revision: Revision::First },
                    text: Text::Literal("stale".into()),
                    author: 2,
                },
                Operation::Incorporate { invocation: 0, items: Items::Saved(1), reverse: false },
                Operation::Withdraw(latest(0)),
                Operation::Move { target: latest(1), before: Some(0) },
                Operation::Move { target: latest(2), before: Some(0) },
                Operation::Reorder(Order::Reverse),
                Operation::Withdraw(latest(1)),
                Operation::Withdraw(latest(0)),
                incorporate(1, 255),
                Operation::Enqueue {
                    id: 0,
                    text: Text::Literal("do not resurrect identity".into()),
                    dependencies: vec![],
                    author: 0,
                },
                Operation::RejectImport(Corruption::ZeroGeneration),
                Operation::RejectImport(Corruption::ZeroRevision),
                Operation::RejectImport(Corruption::ZeroAuthor),
                Operation::RejectImport(Corruption::DuplicateOrder),
            ],
        },
    ]
}

/// Fixed witnesses cover dependencies, immutable edits/corrections, stale captures and bounds.
pub fn examples() -> Vec<Case> {
    let mut cases = transition_examples();
    cases.push(Case {
        initial: vec![
            initial("prerequisite", 0, false),
            initial("dependent", 1, false),
            initial("unrelated", 0, false),
        ],
        operations: vec![
            Operation::Incorporate {
                invocation: 0,
                items: Items::Explicit(vec![
                    latest(1),
                    Target { id: 2, revision: Revision::Future },
                ]),
                reverse: false,
            },
            incorporate(1, 255),
            Operation::Reopen,
        ],
    });
    cases.push(Case {
        initial: vec![],
        operations: vec![
            Operation::Reorder(Order::Reverse),
            incorporate(0, 255),
            Operation::Replies(vec![(0, "reply after empty invocation".into())]),
            Operation::Enqueue {
                id: 0,
                text: Text::Literal("first user content".into()),
                dependencies: vec![],
                author: 1,
            },
            incorporate(0, 255),
            incorporate(1, 255),
            Operation::Reopen,
        ],
    });
    for unicode in [false, true] {
        for delta in [-1, 0, 1] {
            cases.push(Case {
                initial: vec![initial("bounded", 0, false)],
                operations: vec![
                    Operation::Edit {
                        target: latest(0),
                        text: Text::ByteBoundary { delta, unicode },
                        author: 1,
                    },
                    Operation::Capture(0),
                    incorporate(0, 255),
                    Operation::Reopen,
                ],
            });
        }
    }
    cases.push(Case {
        initial: vec![initial("file context prefix", 0, false)],
        operations: vec![
            Operation::FileContext { bytes: 0, unicode: false },
            Operation::FileContext { bytes: 1, unicode: false },
            Operation::FileContext { bytes: 2, unicode: true },
            Operation::FileContext { bytes: MAX_FILE_CONTEXT_SUFFIX_BYTES, unicode: false },
            Operation::FileContext { bytes: MAX_FILE_CONTEXT_SUFFIX_BYTES, unicode: true },
        ],
    });
    cases
}

#[test]
fn praxis_stage3_inputs() {
    crate::local_context::praxis_stage3_support::run(
        "inputs",
        strategy,
        check,
        examples,
        calibration_case,
    );
}

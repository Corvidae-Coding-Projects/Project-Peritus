//! Stateful telemetry discovery against an independent list of record dispositions.

use std::collections::BTreeMap;
use std::num::NonZeroUsize;

use peritus_telemetry::{
    BackpressurePolicy, BufferConfig, BufferCounters, EnqueueOutcome, ExportAck, ExportBatch,
    ExportRecord, ExportStreamId, Exporter, ExporterError, ExporterErrorCode, FlushOutcome,
    MetricName, MetricPoint, RecoveryClass, ShutdownOutcome, TelemetryBuffer, TelemetryError,
    TelemetryErrorKind, TelemetryPump,
};
use peritus_trace::{ObservedTime, TraceId};
use peritus_types::Sha256Digest;
use proptest::prelude::*;
use serde::{Deserialize, Serialize};

/// Fully materialized replay input; no random decisions occur during replay.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub capacity: u8,
    pub batch_size: u8,
    pub policy: Policy,
    pub stream_tag: u8,
    pub operations: Vec<Operation>,
}

/// Both public full-queue policies.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Policy {
    RejectNewest,
    DropOldest,
}

/// One complete public operation and its deterministic dependency response.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum Operation {
    Enqueue {
        value: u16,
    },
    Flush {
        response: Response,
    },
    Shutdown {
        maximum_batches: u8,
        response: Response,
        shutdown_failure: bool,
        /// Successful export calls before the selected response begins; old replays default to zero.
        #[serde(default)]
        accepted_batches_before_response: u8,
    },
    Observe,
}

/// A conforming acknowledgement, explicit exporter failure, or one corrupt ack field.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Response {
    Accept,
    RetryableFailure,
    TerminalFailure,
    WrongStream,
    WrongBatch,
    WrongFirst,
    WrongLast,
    WrongCount,
}

fn response_strategy() -> BoxedStrategy<Response> {
    prop_oneof![
        6 => Just(Response::Accept),
        2 => Just(Response::RetryableFailure),
        1 => Just(Response::TerminalFailure),
        1 => Just(Response::WrongStream),
        1 => Just(Response::WrongBatch),
        1 => Just(Response::WrongFirst),
        1 => Just(Response::WrongLast),
        1 => Just(Response::WrongCount),
    ]
    .boxed()
}

fn operation_strategy(mix: u8) -> BoxedStrategy<Operation> {
    let enqueue_weight = if mix == 0 { 12 } else { 5 };
    let flush_weight = if mix == 1 { 9 } else { 3 };
    prop_oneof![
        enqueue_weight => (0_u16..16).prop_map(|value| Operation::Enqueue { value }),
        flush_weight => response_strategy().prop_map(|response| Operation::Flush { response }),
        2 => (0_u8..5, response_strategy(), any::<bool>(), 0_u8..=3).prop_map(
            |(maximum_batches, response, shutdown_failure, accepted_batches_before_response)| Operation::Shutdown {
                maximum_batches,
                response,
                shutdown_failure,
                accepted_batches_before_response,
            }
        ),
        1 => Just(Operation::Observe),
    ]
    .boxed()
}

/// Generate bounded general histories and capacity-reaching failure/retry/gap patterns.
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    (1_u8..=8, any::<bool>(), 1_u8..=8, 0_u8..3)
        .prop_flat_map(move |(capacity, drop_oldest, stream_tag, mix)| {
            let policy = if drop_oldest { Policy::DropOldest } else { Policy::RejectNewest };
            let general = proptest::collection::vec(operation_strategy(mix), 0..=max_steps);
            let targeted =
                (0_u16..16, proptest::collection::vec(operation_strategy(mix), 0..=max_steps))
                    .prop_map(move |(value, suffix)| {
                        let mut operations = Vec::new();
                        for index in 0..usize::from(capacity) + 2 {
                            operations.push(Operation::Enqueue {
                                value: value.wrapping_add(index as u16),
                            });
                        }
                        operations.extend([
                            Operation::Flush { response: Response::RetryableFailure },
                            Operation::Flush { response: Response::WrongCount },
                            Operation::Flush { response: Response::Accept },
                            Operation::Enqueue { value: value.wrapping_add(100) },
                            Operation::Shutdown {
                                maximum_batches: 0,
                                response: Response::Accept,
                                shutdown_failure: false,
                                accepted_batches_before_response: 0,
                            },
                            Operation::Shutdown {
                                maximum_batches: 8,
                                response: Response::Accept,
                                shutdown_failure: true,
                                accepted_batches_before_response: 0,
                            },
                            Operation::Shutdown {
                                maximum_batches: 8,
                                response: Response::Accept,
                                shutdown_failure: false,
                                accepted_batches_before_response: 0,
                            },
                            Operation::Enqueue { value },
                            Operation::Flush { response: Response::Accept },
                        ]);
                        operations.extend(suffix);
                        operations.truncate(max_steps);
                        operations
                    });
            (1_u8..=capacity, prop_oneof![3 => general, 2 => targeted]).prop_map(
                move |(batch_size, operations)| Case {
                    capacity,
                    batch_size,
                    policy,
                    stream_tag,
                    operations,
                },
            )
        })
        .boxed()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Disposition {
    Pending,
    Exported,
    Dropped,
}

#[derive(Clone, Copy, Debug)]
struct Submission {
    value: u16,
    accepted: bool,
    disposition: Disposition,
}

/// Deliberately unoptimized oracle: all views are recomputed by scanning submissions.
struct Model {
    capacity: usize,
    batch_size: usize,
    policy: Policy,
    submissions: Vec<Submission>,
}

impl Model {
    fn pending(&self) -> Vec<(u64, u16)> {
        self.submissions
            .iter()
            .enumerate()
            .filter(|(_, item)| item.disposition == Disposition::Pending)
            .map(|(index, item)| (index as u64 + 1, item.value))
            .collect()
    }

    fn enqueue(&mut self, value: u16) -> EnqueueOutcome {
        let queue = self.pending();
        let sequence = self.submissions.len() as u64 + 1;
        let full = queue.len() == self.capacity;
        let accepted = !(full && matches!(self.policy, Policy::RejectNewest));
        self.submissions.push(Submission {
            value,
            accepted,
            disposition: if accepted { Disposition::Pending } else { Disposition::Dropped },
        });
        if !accepted {
            EnqueueOutcome::RejectedNewest { rejected_sequence: sequence }
        } else if full {
            let dropped_sequence = queue[0].0;
            self.submissions[dropped_sequence as usize - 1].disposition = Disposition::Dropped;
            EnqueueOutcome::DroppedOldest { accepted_sequence: sequence, dropped_sequence }
        } else {
            EnqueueOutcome::Accepted { sequence }
        }
    }

    fn acknowledge(&mut self, items: &[(u64, u16)]) {
        for (sequence, _) in items {
            self.submissions[*sequence as usize - 1].disposition = Disposition::Exported;
        }
    }

    fn prefix_len(&self) -> usize {
        self.submissions.iter().take_while(|item| item.disposition != Disposition::Pending).count()
    }

    fn counts(&self, through: usize) -> [u64; 4] {
        let prefix = &self.submissions[..through];
        [
            prefix.len() as u64,
            prefix.iter().filter(|item| item.accepted).count() as u64,
            prefix.iter().filter(|item| item.disposition == Disposition::Dropped).count() as u64,
            prefix.iter().filter(|item| item.disposition == Disposition::Exported).count() as u64,
        ]
    }
}

#[derive(Debug)]
struct BatchObservation {
    stream: ExportStreamId,
    id: Sha256Digest,
    bytes: Vec<u8>,
    first: u64,
    last: u64,
    items: Vec<(u64, u16)>,
}

type BatchIdentityLedger = BTreeMap<Vec<(u64, u16)>, (Sha256Digest, Vec<u8>)>;

struct ScriptedExporter {
    response: Response,
    accepted_batches_before_response: usize,
    shutdown_failure: bool,
    batches: Vec<BatchObservation>,
    shutdowns: usize,
    adapter_error: Option<String>,
}

impl ScriptedExporter {
    fn new(response: Response, shutdown_failure: bool) -> Self {
        Self {
            response,
            accepted_batches_before_response: 0,
            shutdown_failure,
            batches: Vec::new(),
            shutdowns: 0,
            adapter_error: None,
        }
    }

    fn response_at(&self, ordinal: usize) -> Response {
        if ordinal < self.accepted_batches_before_response {
            Response::Accept
        } else {
            self.response
        }
    }
}

impl Exporter for ScriptedExporter {
    fn export(&mut self, batch: &ExportBatch) -> Result<ExportAck, ExporterError> {
        let observed = (|| {
            let items = batch
                .items()
                .iter()
                .map(|item| {
                    let ExportRecord::Metric(point) = item.record() else {
                        return Err("fixture record changed its variant".to_owned());
                    };
                    if point.name() != MetricName::ToolDispatches
                        || point.time().unix_nanos() != 1
                        || point.time().monotonic_tick() != 1
                        || point.trace_id().as_bytes() != &[1; 16]
                    {
                        return Err(format!(
                            "fixture metric metadata changed: name={:?}, wall={}, tick={}, trace={:?}",
                            point.name(),
                            point.time().unix_nanos(),
                            point.time().monotonic_tick(),
                            point.trace_id().as_bytes(),
                        ));
                    }
                    let value = u16::try_from(point.value())
                        .map_err(|error| format!("fixture value changed: {error}"))?;
                    Ok((item.sequence(), value))
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok::<_, String>(BatchObservation {
                stream: batch.stream_id(),
                id: batch.batch_id(),
                bytes: batch.canonical_bytes().map_err(|error| error.to_string())?,
                first: batch.first_sequence(),
                last: batch.last_sequence(),
                items,
            })
        })();
        match observed {
            Ok(observation) => self.batches.push(observation),
            Err(error) => {
                self.adapter_error = Some(error);
                return Err(ExporterError::new(ExporterErrorCode::Protocol, false));
            }
        }
        match self.response_at(self.batches.len() - 1) {
            Response::Accept => Ok(ExportAck::accept(batch)),
            Response::RetryableFailure => {
                Err(ExporterError::new(ExporterErrorCode::Unavailable, true))
            }
            Response::TerminalFailure => {
                Err(ExporterError::new(ExporterErrorCode::Rejected, false))
            }
            response => {
                let mut stream = batch.stream_id();
                let mut digest = batch.batch_id();
                let mut first = batch.first_sequence();
                let mut last = batch.last_sequence();
                let mut count = batch.len();
                match response {
                    Response::WrongStream => {
                        let mut bytes = *stream.as_bytes();
                        bytes[15] ^= 0x80;
                        stream = ExportStreamId::new(bytes)
                            .map_err(|_| ExporterError::new(ExporterErrorCode::Protocol, false))?;
                    }
                    Response::WrongBatch => {
                        let mut bytes = *digest.as_bytes();
                        bytes[0] ^= 0x80;
                        digest = Sha256Digest::new(bytes);
                    }
                    Response::WrongFirst => first += 1,
                    Response::WrongLast => last += 1,
                    Response::WrongCount => count += 1,
                    Response::Accept | Response::RetryableFailure | Response::TerminalFailure => {
                        unreachable!("handled above")
                    }
                }
                Ok(ExportAck::new(stream, digest, first, last, count))
            }
        }
    }

    fn shutdown(&mut self) -> Result<(), ExporterError> {
        self.shutdowns += 1;
        if self.shutdown_failure {
            Err(ExporterError::new(ExporterErrorCode::Shutdown, false))
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ExpectedError {
    AckMismatch,
    Exporter { code: ExporterErrorCode, retryable: bool },
}

fn response_error(response: Response) -> Option<ExpectedError> {
    match response {
        Response::Accept => None,
        Response::RetryableFailure => {
            Some(ExpectedError::Exporter { code: ExporterErrorCode::Unavailable, retryable: true })
        }
        Response::TerminalFailure => {
            Some(ExpectedError::Exporter { code: ExporterErrorCode::Rejected, retryable: false })
        }
        Response::WrongStream
        | Response::WrongBatch
        | Response::WrongFirst
        | Response::WrongLast
        | Response::WrongCount => Some(ExpectedError::AckMismatch),
    }
}

fn compare_result<T: std::fmt::Debug + PartialEq>(
    actual: Result<T, TelemetryError>,
    expected: Result<T, ExpectedError>,
) -> Result<(), String> {
    match (actual, expected) {
        (Ok(actual), Ok(expected)) if actual == expected => Ok(()),
        (Err(error), Err(ExpectedError::AckMismatch))
            if error.kind() == TelemetryErrorKind::AckMismatch
                && error.recovery() == RecoveryClass::CorrectInput =>
        {
            Ok(())
        }
        (Err(error), Err(ExpectedError::Exporter { code, retryable }))
            if error.kind() == TelemetryErrorKind::ExportFailed
                && error.exporter_code() == Some(code)
                && error.exporter_retryable() == Some(retryable)
                && error.recovery()
                    == if retryable {
                        RecoveryClass::RetryExporter
                    } else {
                        RecoveryClass::TerminalExporter
                    } =>
        {
            Ok(())
        }
        (actual, expected) => Err(format!("operation result {actual:?}, expected {expected:?}")),
    }
}

fn counts(actual: BufferCounters) -> [u64; 4] {
    [actual.submitted(), actual.accepted(), actual.dropped(), actual.exported()]
}

fn compare_state(pump: &TelemetryPump, model: &Model) -> Result<(), String> {
    let pending = model.pending().len();
    if pump.buffer().len() != pending || pump.buffer().is_empty() != (pending == 0) {
        return Err(format!(
            "queue length/emptiness {:?}/{}, expected {pending}/{}",
            pump.buffer().len(),
            pump.buffer().is_empty(),
            pending == 0
        ));
    }
    let expected = model.counts(model.submissions.len());
    if counts(pump.buffer().counters()) != expected {
        return Err(format!(
            "global counters {:?}, expected {expected:?}",
            counts(pump.buffer().counters())
        ));
    }
    let prefix = model.prefix_len();
    if pump.disposed_through_sequence() != prefix as u64 {
        return Err(format!(
            "disposed prefix {}, expected {prefix}",
            pump.disposed_through_sequence()
        ));
    }
    let expected = model.counts(prefix);
    if counts(pump.disposed_counters()) != expected {
        return Err(format!(
            "disposed counters {:?}, expected {expected:?}",
            counts(pump.disposed_counters())
        ));
    }
    Ok(())
}

fn record(value: u16) -> Result<ExportRecord, String> {
    Ok(ExportRecord::Metric(MetricPoint::new(
        MetricName::ToolDispatches,
        u64::from(value),
        ObservedTime::new(1, 1).map_err(|error| error.to_string())?,
        TraceId::new([1; 16]).map_err(|error| error.to_string())?,
    )))
}

fn bump(metrics: &mut crate::Metrics, name: &str, amount: u64) {
    *metrics.entry(name.to_owned()).or_default() += amount;
}

fn inspect_exporter(
    exporter: &ScriptedExporter,
    expected: &[Vec<(u64, u16)>],
    shutdowns: usize,
    stream: ExportStreamId,
    identities: &mut BatchIdentityLedger,
    metrics: &mut crate::Metrics,
) -> Result<(), String> {
    if let Some(error) = &exporter.adapter_error {
        return Err(format!("export adapter could not observe batch: {error}"));
    }
    if exporter.shutdowns != shutdowns || exporter.batches.len() != expected.len() {
        return Err(format!(
            "export/shutdown calls {}/{}, expected {}/{}",
            exporter.batches.len(),
            exporter.shutdowns,
            expected.len(),
            shutdowns
        ));
    }
    for (ordinal, (actual, expected)) in exporter.batches.iter().zip(expected).enumerate() {
        if actual.stream != stream
            || actual.items != *expected
            || actual.first != expected.first().map_or(0, |item| item.0)
            || actual.last != expected.last().map_or(0, |item| item.0)
        {
            return Err(format!("exported batch {actual:?}, expected items {expected:?}"));
        }
        if let Some((id, bytes)) = identities.get(expected) {
            if *id != actual.id || *bytes != actual.bytes {
                return Err("unchanged batch changed identity or encoded bytes after retry".into());
            }
            bump(metrics, "repeated_batch_identity_checks", 1);
        } else {
            identities.insert(expected.clone(), (actual.id, actual.bytes.clone()));
        }
        bump(metrics, "exporter_calls", 1);
        bump(
            metrics,
            match exporter.response_at(ordinal) {
                Response::Accept => "responses_accept",
                Response::RetryableFailure => "responses_retryable_failure",
                Response::TerminalFailure => "responses_terminal_failure",
                Response::WrongStream => "responses_wrong_stream",
                Response::WrongBatch => "responses_wrong_batch",
                Response::WrongFirst => "responses_wrong_first",
                Response::WrongLast => "responses_wrong_last",
                Response::WrongCount => "responses_wrong_count",
            },
            1,
        );
        if expected.windows(2).any(|items| items[1].0 != items[0].0 + 1) {
            bump(metrics, "noncontiguous_sequence_batches", 1);
        }
    }
    bump(metrics, "exporter_shutdown_calls", shutdowns as u64);
    Ok(())
}

/// Replay against the actual public queue/pump APIs and a fully separate list model.
pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    let capacity = NonZeroUsize::new(usize::from(case.capacity))
        .ok_or_else(|| "case capacity must be positive".to_owned())?;
    let batch_size = NonZeroUsize::new(usize::from(case.batch_size))
        .ok_or_else(|| "case batch size must be positive".to_owned())?;
    let config = BufferConfig::new(
        capacity,
        batch_size,
        match case.policy {
            Policy::RejectNewest => BackpressurePolicy::RejectNewest,
            Policy::DropOldest => BackpressurePolicy::DropOldest,
        },
    )
    .map_err(|error| error.to_string())?;
    let stream = ExportStreamId::new([case.stream_tag; 16]).map_err(|error| error.to_string())?;
    let mut pump = TelemetryPump::new(stream, TelemetryBuffer::new(config));
    let mut model = Model {
        capacity: capacity.get(),
        batch_size: batch_size.get(),
        policy: case.policy,
        submissions: Vec::new(),
    };
    let mut metrics = crate::Metrics::new();
    let mut identities = BTreeMap::new();
    compare_state(&pump, &model)?;
    for (step, operation) in case.operations.iter().enumerate() {
        let replay = (|| {
            match *operation {
                Operation::Enqueue { value } => {
                    let expected = model.enqueue(value);
                    let actual = pump
                        .buffer_mut()
                        .enqueue(record(value)?)
                        .map_err(|error| format!("enqueue failed: {error}"))?;
                    if actual != expected {
                        return Err(format!("enqueue {actual:?}, expected {expected:?}"));
                    }
                    let class = match expected {
                        EnqueueOutcome::Accepted { .. } => "accepted_without_eviction",
                        EnqueueOutcome::DroppedOldest { .. } => "oldest_evictions",
                        EnqueueOutcome::RejectedNewest { .. } => "newest_rejections",
                    };
                    bump(&mut metrics, class, 1);
                    bump(&mut metrics, "enqueues", 1);
                }
                Operation::Flush { response } => {
                    let items =
                        model.pending().into_iter().take(model.batch_size).collect::<Vec<_>>();
                    let mut exporter = ScriptedExporter::new(response, false);
                    let expected = if items.is_empty() {
                        bump(&mut metrics, "empty_flushes", 1);
                        Ok(FlushOutcome::Empty)
                    } else if let Some(error) = response_error(response) {
                        bump(&mut metrics, "failed_flushes", 1);
                        Err(error)
                    } else {
                        model.acknowledge(&items);
                        bump(&mut metrics, "successful_flushes", 1);
                        Ok(FlushOutcome::Exported {
                            count: items.len() as u64,
                            through_sequence: items.last().map_or(0, |item| item.0),
                        })
                    };
                    compare_result(pump.flush_one(&mut exporter), expected)?;
                    let expected_batches = if items.is_empty() { Vec::new() } else { vec![items] };
                    inspect_exporter(
                        &exporter,
                        &expected_batches,
                        0,
                        stream,
                        &mut identities,
                        &mut metrics,
                    )?;
                    bump(&mut metrics, "flushes", 1);
                }
                Operation::Shutdown {
                    maximum_batches,
                    response,
                    shutdown_failure,
                    accepted_batches_before_response,
                } => {
                    let mut expected_batches = Vec::new();
                    let mut failure = None;
                    for ordinal in 0..maximum_batches {
                        let items =
                            model.pending().into_iter().take(model.batch_size).collect::<Vec<_>>();
                        if items.is_empty() {
                            break;
                        }
                        expected_batches.push(items.clone());
                        let expected_response = if ordinal < accepted_batches_before_response {
                            Response::Accept
                        } else {
                            response
                        };
                        if let Some(error) = response_error(expected_response) {
                            if ordinal > 0 {
                                bump(&mut metrics, "shutdown_failures_after_progress", 1);
                            }
                            failure = Some(error);
                            break;
                        }
                        model.acknowledge(&items);
                    }
                    let mut shutdowns = 0;
                    let expected = if let Some(error) = failure {
                        Err(error)
                    } else if !model.pending().is_empty() {
                        bump(&mut metrics, "bounded_shutdowns_pending", 1);
                        Ok(ShutdownOutcome::Pending { remaining: model.pending().len() })
                    } else {
                        shutdowns = 1;
                        if shutdown_failure {
                            bump(&mut metrics, "exporter_shutdown_failures", 1);
                            Err(ExpectedError::Exporter {
                                code: ExporterErrorCode::Shutdown,
                                retryable: false,
                            })
                        } else {
                            bump(&mut metrics, "completed_shutdowns", 1);
                            Ok(ShutdownOutcome::Complete)
                        }
                    };
                    let mut exporter = ScriptedExporter::new(response, shutdown_failure);
                    exporter.accepted_batches_before_response =
                        usize::from(accepted_batches_before_response);
                    compare_result(
                        pump.shutdown(&mut exporter, u64::from(maximum_batches)),
                        expected,
                    )?;
                    inspect_exporter(
                        &exporter,
                        &expected_batches,
                        shutdowns,
                        stream,
                        &mut identities,
                        &mut metrics,
                    )?;
                    bump(&mut metrics, "shutdowns", 1);
                }
                Operation::Observe => bump(&mut metrics, "explicit_observations", 1),
            }
            compare_state(&pump, &model)?;
            let pending = model.pending().len() as u64;
            let previous = metrics.get("maximum_pending").copied().unwrap_or(0);
            metrics.insert("maximum_pending".into(), previous.max(pending));
            if pump.disposed_through_sequence() < pump.buffer().counters().submitted()
                && pump.buffer().counters().dropped() > pump.disposed_counters().dropped()
            {
                bump(&mut metrics, "drops_beyond_unresolved_prefix", 1);
            }
            Ok::<_, String>(())
        })();
        replay.map_err(|error| format!("step {step} {operation:?}: {error}"))?;
        bump(&mut metrics, "operations", 1);
    }
    // Observe full pending contents through an independent clone, without changing the replay.
    let mut cloned = TelemetryPump::new(stream, pump.buffer().clone());
    let mut exporter = ScriptedExporter::new(Response::Accept, false);
    let expected = model.pending();
    for _ in 0..=model.capacity {
        if cloned.buffer().is_empty() {
            break;
        }
        cloned.flush_one(&mut exporter).map_err(|error| format!("clone drain: {error}"))?;
    }
    if !cloned.buffer().is_empty() {
        return Err("clone drain made insufficient progress within its capacity bound".into());
    }
    let actual =
        exporter.batches.iter().flat_map(|batch| batch.items.iter().copied()).collect::<Vec<_>>();
    if actual != expected {
        return Err(format!("retained queue items {actual:?}, expected {expected:?}"));
    }
    compare_state(&pump, &model)?;
    bump(&mut metrics, "clone_queue_checks", 1);
    bump(
        &mut metrics,
        match case.policy {
            Policy::RejectNewest => "policy_reject_newest",
            Policy::DropOldest => "policy_drop_oldest",
        },
        1,
    );
    Ok(metrics)
}

/// Deterministic histories establish rejection gaps, eviction, retries, ack fields, and shutdown.
pub fn examples() -> Vec<Case> {
    let mut cases = vec![calibration_case()];
    for policy in [Policy::RejectNewest, Policy::DropOldest] {
        cases.push(Case {
            capacity: 2,
            batch_size: 1,
            policy,
            stream_tag: 1,
            operations: vec![
                Operation::Flush { response: Response::Accept },
                Operation::Enqueue { value: 1 },
                Operation::Enqueue { value: 2 },
                Operation::Enqueue { value: 3 },
                Operation::Flush { response: Response::RetryableFailure },
                Operation::Flush { response: Response::Accept },
                Operation::Enqueue { value: 4 },
                Operation::Flush { response: Response::Accept },
                Operation::Shutdown {
                    maximum_batches: 0,
                    response: Response::Accept,
                    shutdown_failure: false,
                    accepted_batches_before_response: 0,
                },
                Operation::Shutdown {
                    maximum_batches: 4,
                    response: Response::Accept,
                    shutdown_failure: true,
                    accepted_batches_before_response: 0,
                },
                Operation::Shutdown {
                    maximum_batches: 0,
                    response: Response::Accept,
                    shutdown_failure: false,
                    accepted_batches_before_response: 0,
                },
                Operation::Enqueue { value: 5 },
                Operation::Flush { response: Response::Accept },
            ],
        });
        for response in [
            Response::RetryableFailure,
            Response::TerminalFailure,
            Response::WrongStream,
            Response::WrongBatch,
            Response::WrongFirst,
            Response::WrongLast,
            Response::WrongCount,
        ] {
            cases.push(Case {
                capacity: 3,
                batch_size: 2,
                policy,
                stream_tag: 2,
                operations: vec![
                    Operation::Enqueue { value: 1 },
                    Operation::Enqueue { value: 1 },
                    Operation::Flush { response },
                    Operation::Shutdown {
                        maximum_batches: 3,
                        response,
                        shutdown_failure: false,
                        accepted_batches_before_response: 0,
                    },
                    Operation::Flush { response: Response::Accept },
                    Operation::Shutdown {
                        maximum_batches: 0,
                        response: Response::Accept,
                        shutdown_failure: false,
                        accepted_batches_before_response: 0,
                    },
                ],
            });
        }
        cases.push(Case {
            capacity: 4,
            batch_size: 1,
            policy,
            stream_tag: 3,
            operations: vec![
                Operation::Enqueue { value: 1 },
                Operation::Enqueue { value: 2 },
                Operation::Enqueue { value: 3 },
                Operation::Enqueue { value: 4 },
                Operation::Shutdown {
                    maximum_batches: 4,
                    response: Response::RetryableFailure,
                    shutdown_failure: false,
                    accepted_batches_before_response: 1,
                },
                Operation::Flush { response: Response::WrongCount },
                Operation::Shutdown {
                    maximum_batches: 2,
                    response: Response::Accept,
                    shutdown_failure: false,
                    accepted_batches_before_response: 0,
                },
                Operation::Shutdown {
                    maximum_batches: 1,
                    response: Response::Accept,
                    shutdown_failure: false,
                    accepted_batches_before_response: 0,
                },
            ],
        });
    }
    cases
}

/// Small witness for the dropped-counter calibration mutant.
pub fn calibration_case() -> Case {
    Case {
        capacity: 1,
        batch_size: 1,
        policy: Policy::RejectNewest,
        stream_tag: 1,
        operations: vec![Operation::Enqueue { value: 1 }, Operation::Enqueue { value: 2 }],
    }
}

//! Independent queue and disposition model for bounded telemetry export histories.

use std::{collections::VecDeque, num::NonZeroUsize};

use peritus_telemetry::{
    BackpressurePolicy, BufferConfig, CheckpointStore, EnqueueOutcome, ExportAck, ExportBatch,
    ExportCheckpoint, ExportRecord, ExportStreamId, Exporter, ExporterError, ExporterErrorCode,
    FlushOutcome, MetricName, MetricPoint, ShutdownOutcome, TelemetryBuffer, TelemetryErrorKind,
    TelemetryPump, project_telemetry, recover_buffer,
};
use peritus_trace::{
    CausalBinding, DiagnosticCode, Observation, ObservationKind, ObservedTime, SpanId, SpanKind,
    TraceId, TraceProjectionState,
};
use peritus_types::{EventId, SessionId, Sha256Digest};
use proptest::prelude::*;
use proptest::test_runner::{FileFailurePersistence, TestCaseError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Disposition {
    Pending,
    Exported,
    Evicted,
    Rejected,
}

#[derive(Debug, Default)]
struct Model {
    items: Vec<(u64, Disposition)>,
    queue: VecDeque<(u64, u64)>,
}

impl Model {
    fn enqueue(
        &mut self,
        value: u64,
        capacity: usize,
        policy: BackpressurePolicy,
    ) -> EnqueueOutcome {
        let sequence = u64::try_from(self.items.len()).expect("small generated history") + 1;
        if self.queue.len() == capacity {
            match policy {
                BackpressurePolicy::RejectNewest => {
                    self.items.push((value, Disposition::Rejected));
                    return EnqueueOutcome::RejectedNewest { rejected_sequence: sequence };
                }
                BackpressurePolicy::DropOldest => {
                    let (dropped_sequence, _) = self.queue.pop_front().expect("full queue");
                    self.items[usize::try_from(dropped_sequence - 1).expect("small sequence")].1 =
                        Disposition::Evicted;
                    self.queue.push_back((sequence, value));
                    self.items.push((value, Disposition::Pending));
                    return EnqueueOutcome::DroppedOldest {
                        accepted_sequence: sequence,
                        dropped_sequence,
                    };
                }
            }
        }
        self.queue.push_back((sequence, value));
        self.items.push((value, Disposition::Pending));
        EnqueueOutcome::Accepted { sequence }
    }

    fn counts(&self, prefix: usize) -> (u64, u64, u64, u64) {
        let mut accepted = 0;
        let mut dropped = 0;
        let mut exported = 0;
        for (_, disposition) in self.items.iter().take(prefix) {
            if *disposition != Disposition::Rejected {
                accepted += 1;
            }
            if matches!(disposition, Disposition::Evicted | Disposition::Rejected) {
                dropped += 1;
            }
            if *disposition == Disposition::Exported {
                exported += 1;
            }
        }
        (u64::try_from(prefix).expect("small history"), accepted, dropped, exported)
    }

    fn disposed(&self) -> usize {
        self.items
            .iter()
            .position(|(_, status)| *status == Disposition::Pending)
            .unwrap_or(self.items.len())
    }

    fn assert_matches(&self, pump: &TelemetryPump) {
        let (submitted, accepted, dropped, exported) = self.counts(self.items.len());
        let counters = pump.buffer().counters();
        assert_eq!(
            (counters.submitted(), counters.accepted(), counters.dropped(), counters.exported()),
            (submitted, accepted, dropped, exported)
        );
        assert_eq!(pump.buffer().len(), self.queue.len());
        assert_eq!(submitted, u64::try_from(self.queue.len()).unwrap() + dropped + exported);
        let disposed = self.disposed();
        let boundary = pump.disposed_counters();
        let expected = self.counts(disposed);
        assert_eq!(pump.disposed_through_sequence(), expected.0);
        assert_eq!(
            (boundary.submitted(), boundary.accepted(), boundary.dropped(), boundary.exported()),
            expected
        );
    }
}

#[derive(Clone, Copy)]
enum Mode {
    Accept,
    Fail,
    WrongAck(u8),
}

struct Probe {
    mode: Mode,
    observed: Option<ExportBatch>,
    shutdown_calls: usize,
}

impl Exporter for Probe {
    fn export(&mut self, batch: &ExportBatch) -> Result<ExportAck, ExporterError> {
        self.observed = Some(batch.clone());
        match self.mode {
            Mode::Accept => Ok(ExportAck::accept(batch)),
            Mode::Fail => Err(ExporterError::new(ExporterErrorCode::Unavailable, true)),
            Mode::WrongAck(field) => Ok(ExportAck::new(
                if field == 0 { ExportStreamId::new([9; 16]).unwrap() } else { batch.stream_id() },
                if field == 1 {
                    let mut bytes = *batch.batch_id().as_bytes();
                    bytes[0] ^= 1;
                    Sha256Digest::new(bytes)
                } else {
                    batch.batch_id()
                },
                batch.first_sequence() + u64::from(field == 2),
                batch.last_sequence() + u64::from(field == 3),
                batch.len() + usize::from(field == 4),
            )),
        }
    }

    fn shutdown(&mut self) -> Result<(), ExporterError> {
        self.shutdown_calls += 1;
        Ok(())
    }
}

fn record(sequence: u64, value: u64) -> ExportRecord {
    ExportRecord::Metric(MetricPoint::new(
        MetricName::ProviderRequests,
        value,
        ObservedTime::new(1_000 + sequence, 1_000 + sequence).unwrap(),
        TraceId::new([1; 16]).unwrap(),
    ))
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 384,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "tests/proptest_buffer.proptest-regressions"
        ))),
        .. ProptestConfig::default()
    })]
    #[test]
    fn generated_queue_ack_and_checkpoint_histories(
        capacity in 1usize..=6,
        requested_batch in 1usize..=6,
        drop_oldest in any::<bool>(),
        operations in prop::collection::vec((0u8..6, any::<u8>()), 1..140),
    ) {
        let policy = if drop_oldest { BackpressurePolicy::DropOldest } else { BackpressurePolicy::RejectNewest };
        let batch_size = requested_batch.min(capacity);
        let config = BufferConfig::new(NonZeroUsize::new(capacity).unwrap(),
            NonZeroUsize::new(batch_size).unwrap(), policy).unwrap();
        let stream = ExportStreamId::new([2; 16]).unwrap();
        let mut pump = TelemetryPump::new(stream, TelemetryBuffer::new(config));
        let mut model = Model::default();
        let directory = tempfile::tempdir().unwrap();
        let store = CheckpointStore::open(directory.path(), stream, NonZeroUsize::new(8).unwrap()).unwrap();
        for (index, (operation, payload)) in operations.into_iter().enumerate() {
            match operation {
                0 | 1 => {
                    let sequence = u64::try_from(model.items.len()).unwrap() + 1;
                    let value = sequence * 256 + u64::from(payload);
                    let expected = model.enqueue(value, capacity, policy);
                    let observed = pump.buffer_mut().enqueue(record(sequence, value)).unwrap();
                    prop_assert_eq!(observed, expected, "operation {}", index);
                }
                2 | 3 => {
                    let mode = match payload % 7 { 0 => Mode::Accept, 1 => Mode::Fail, field => Mode::WrongAck(field - 2) };
                    let mut probe = Probe { mode, observed: None, shutdown_calls: 0 };
                    let pending = model.queue.iter().take(batch_size).copied().collect::<Vec<_>>();
                    let outcome = pump.flush_one(&mut probe);
                    if pending.is_empty() {
                        prop_assert!(probe.observed.is_none());
                        prop_assert_eq!(outcome.unwrap(), FlushOutcome::Empty);
                    } else {
                        let batch = probe.observed.expect("nonempty queue reached exporter");
                        let actual = batch.items().iter().map(|item| {
                            let ExportRecord::Metric(point) = item.record() else { panic!("non-metric record") };
                            (item.sequence(), point.value())
                        }).collect::<Vec<_>>();
                        prop_assert_eq!(actual, pending.clone(), "operation {}", index);
                        match mode {
                            Mode::Accept => {
                                prop_assert_eq!(outcome.unwrap(), FlushOutcome::Exported {
                                    count: u64::try_from(pending.len()).unwrap(),
                                    through_sequence: pending.last().unwrap().0,
                                });
                                for (sequence, _) in pending {
                                    model.queue.pop_front();
                                    model.items[usize::try_from(sequence - 1).unwrap()].1 = Disposition::Exported;
                                }
                            }
                            Mode::Fail => prop_assert_eq!(outcome.unwrap_err().kind(), TelemetryErrorKind::ExportFailed),
                            Mode::WrongAck(_) => prop_assert_eq!(outcome.unwrap_err().kind(), TelemetryErrorKind::AckMismatch),
                        }
                    }
                }
                4 => {
                    let checkpoint = ExportCheckpoint::from_pump(&pump);
                    store.persist(checkpoint).unwrap();
                    prop_assert_eq!(store.load_latest().unwrap(), Some(checkpoint), "operation {}", index);
                }
                _ => {
                    let maximum = u64::from(payload % 4);
                    let mut probe = Probe { mode: Mode::Accept, observed: None, shutdown_calls: 0 };
                    let outcome = pump.shutdown(&mut probe, maximum).unwrap();
                    for _ in 0..maximum {
                        if model.queue.is_empty() { break; }
                        for _ in 0..batch_size {
                            let Some((sequence, _)) = model.queue.pop_front() else { break };
                            model.items[usize::try_from(sequence - 1).unwrap()].1 = Disposition::Exported;
                        }
                    }
                    if model.queue.is_empty() {
                        prop_assert_eq!(outcome, ShutdownOutcome::Complete);
                        prop_assert_eq!(probe.shutdown_calls, 1);
                    } else {
                        prop_assert_eq!(outcome, ShutdownOutcome::Pending { remaining: model.queue.len() });
                        prop_assert_eq!(probe.shutdown_calls, 0);
                    }
                }
            }
            model.assert_matches(&pump);
        }
    }
}

fn trace_projection(diagnostics: usize) -> peritus_telemetry::TelemetryProjection {
    let trace = TraceId::new([3; 16]).unwrap();
    let span = SpanId::new([4; 8]).unwrap();
    let binding = CausalBinding::session(SessionId::new([5; 16]).unwrap());
    let mut state = TraceProjectionState::default();
    let mut predecessor = None;
    for ordinal in 0..=diagnostics {
        let mut bytes = [0_u8; 16];
        bytes[0] = 6;
        bytes[8..].copy_from_slice(&(u64::try_from(ordinal).unwrap() + 1).to_be_bytes());
        let event = EventId::new(bytes).unwrap();
        let kind = if ordinal == 0 {
            ObservationKind::SpanStarted(SpanKind::Internal)
        } else {
            ObservationKind::Diagnostic(DiagnosticCode::ProviderRequestStarted)
        };
        let observation = Observation::new(
            event,
            trace,
            span,
            u64::try_from(ordinal).unwrap() + 1,
            None,
            predecessor.into_iter().collect(),
            binding,
            ObservedTime::new(
                1_000 + u64::try_from(ordinal).unwrap(),
                1_000 + u64::try_from(ordinal).unwrap(),
            )
            .unwrap(),
            kind,
            Vec::new(),
            Vec::new(),
        )
        .unwrap();
        state.apply(observation, u64::try_from(ordinal).unwrap() + 1).unwrap();
        predecessor = Some(event);
    }
    let projection = project_telemetry(&state).unwrap();
    assert_eq!(projection.records().len(), diagnostics * 2);
    assert_eq!(
        projection.metrics().value(MetricName::ProviderRequests),
        u64::try_from(diagnostics).unwrap()
    );
    for (index, pair) in projection.records().chunks_exact(2).enumerate() {
        let ExportRecord::Event(event) = &pair[0] else {
            panic!("each diagnostic first projects an event")
        };
        assert_eq!(event.code(), DiagnosticCode::ProviderRequestStarted);
        let ExportRecord::Metric(point) = &pair[1] else {
            panic!("each provider-start diagnostic projects one metric")
        };
        assert_eq!(point.name(), MetricName::ProviderRequests);
        assert_eq!(point.value(), u64::try_from(index).unwrap() + 1);
    }
    projection
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 128,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "tests/proptest_buffer.proptest-regressions"
        ))),
        .. ProptestConfig::default()
    })]
    #[test]
    fn generated_trace_checkpoint_restarts(
        diagnostics in 1usize..40,
        capacity in 1usize..=6,
        requested_batch in 1usize..=6,
        drop_oldest in any::<bool>(),
        initial_selector in 0usize..100,
        flush_period in 0usize..12,
    ) {
        let projection = trace_projection(diagnostics);
        let records = projection.records();
        let initial_count = initial_selector % (records.len() + 1);
        let policy = if drop_oldest { BackpressurePolicy::DropOldest } else { BackpressurePolicy::RejectNewest };
        let config = BufferConfig::new(NonZeroUsize::new(capacity).unwrap(),
            NonZeroUsize::new(requested_batch.min(capacity)).unwrap(), policy).unwrap();
        let stream = ExportStreamId::new([7; 16]).unwrap();
        let mut first = TelemetryPump::new(stream, TelemetryBuffer::new(config));
        for (index, record) in records.iter().take(initial_count).enumerate() {
            first.buffer_mut().enqueue(record.clone()).unwrap();
            if flush_period != 0 && index % flush_period == 0 {
                let mut exporter = Probe { mode: Mode::Accept, observed: None, shutdown_calls: 0 };
                first.flush_one(&mut exporter).unwrap();
            }
        }
        let checkpoint = ExportCheckpoint::from_pump(&first);
        let directory = tempfile::tempdir().unwrap();
        let store = CheckpointStore::open(directory.path(), stream, NonZeroUsize::new(8).unwrap()).unwrap();
        store.persist(checkpoint).unwrap();
        let loaded = store.load_latest().unwrap().unwrap();
        prop_assert_eq!(loaded, checkpoint);
        let report = recover_buffer(config, stream, Some(loaded), &projection).unwrap();
        let prefix = usize::try_from(checkpoint.disposed_through_sequence()).unwrap();
        prop_assert_eq!(report.replayed(), u64::try_from(records.len() - prefix).unwrap());
        let actual_recovery_drops = report.dropped_during_recovery();
        let mut recovered = report.into_pump();

        // Independent replay from the final-disposition boundary: later in-memory effects
        // may be repeated after restart, but the saved prefix cannot be repeated or skipped.
        let mut status = Vec::new();
        let mut queue = VecDeque::new();
        let mut accepted = checkpoint.counters().accepted();
        let mut dropped = checkpoint.counters().dropped();
        let exported = checkpoint.counters().exported();
        let mut recovery_drops = 0_u64;
        for index in prefix..records.len() {
            if queue.len() == capacity {
                recovery_drops += 1;
                dropped += 1;
                if policy == BackpressurePolicy::RejectNewest {
                    status.push(Disposition::Rejected);
                    continue;
                }
                let old = queue.pop_front().unwrap();
                status[old - prefix] = Disposition::Evicted;
            }
            queue.push_back(index);
            status.push(Disposition::Pending);
            accepted += 1;
        }
        prop_assert_eq!(actual_recovery_drops, recovery_drops);
        let counters = recovered.buffer().counters();
        prop_assert_eq!((counters.submitted(), counters.accepted(), counters.dropped(), counters.exported()),
            (u64::try_from(records.len()).unwrap(), accepted, dropped, exported));
        prop_assert_eq!(recovered.buffer().len(), queue.len());
        let disposed_tail = status.iter().position(|value| *value == Disposition::Pending).unwrap_or(status.len());
        let disposed = prefix + disposed_tail;
        prop_assert_eq!(recovered.disposed_through_sequence(), u64::try_from(disposed).unwrap());
        let boundary = recovered.disposed_counters();
        let tail_accepted = status[..disposed_tail].iter().filter(|value| **value != Disposition::Rejected).count();
        let tail_dropped = status[..disposed_tail].iter().filter(|value| matches!(value, Disposition::Rejected | Disposition::Evicted)).count();
        prop_assert_eq!((boundary.submitted(), boundary.accepted(), boundary.dropped(), boundary.exported()),
            (u64::try_from(disposed).unwrap(), checkpoint.counters().accepted() + u64::try_from(tail_accepted).unwrap(),
             checkpoint.counters().dropped() + u64::try_from(tail_dropped).unwrap(), exported));
        let mut exporter = Probe { mode: Mode::Accept, observed: None, shutdown_calls: 0 };
        let next = recovered.flush_one(&mut exporter).unwrap();
        if queue.is_empty() {
            prop_assert_eq!(next, FlushOutcome::Empty);
            prop_assert!(exporter.observed.is_none());
        } else {
            let batch = exporter.observed.unwrap();
            let expected = queue.iter().take(config.batch_size()).copied().collect::<Vec<_>>();
            let actual = batch.items().iter().map(|item| {
                let index = usize::try_from(item.sequence() - 1).unwrap();
                prop_assert_eq!(item.record(), &records[index]);
                Ok(index)
            }).collect::<Result<Vec<_>, TestCaseError>>()?;
            prop_assert_eq!(&actual, &expected);
            prop_assert_eq!(next, FlushOutcome::Exported {
                count: u64::try_from(expected.len()).unwrap(),
                through_sequence: u64::try_from(*expected.last().unwrap()).unwrap() + 1,
            });
        }
    }
}

#[test]
fn every_ack_identity_field_is_checked_before_dequeue_and_retry() {
    for field in 0..5 {
        let config = BufferConfig::new(
            NonZeroUsize::new(3).unwrap(),
            NonZeroUsize::new(2).unwrap(),
            BackpressurePolicy::RejectNewest,
        )
        .unwrap();
        let mut pump =
            TelemetryPump::new(ExportStreamId::new([2; 16]).unwrap(), TelemetryBuffer::new(config));
        pump.buffer_mut().enqueue(record(1, 73)).unwrap();
        pump.buffer_mut().enqueue(record(2, 91)).unwrap();
        let counters = pump.buffer().counters();
        let mut bad = Probe { mode: Mode::WrongAck(field), observed: None, shutdown_calls: 0 };
        assert_eq!(pump.flush_one(&mut bad).unwrap_err().kind(), TelemetryErrorKind::AckMismatch);
        assert_eq!(pump.buffer().len(), 2);
        assert_eq!(pump.buffer().counters(), counters);
        let mut retry = Probe { mode: Mode::Accept, observed: None, shutdown_calls: 0 };
        assert_eq!(
            pump.flush_one(&mut retry).unwrap(),
            FlushOutcome::Exported { count: 2, through_sequence: 2 }
        );
        assert_eq!(
            retry.observed, bad.observed,
            "retry changed exact pending batch for field {field}"
        );
        assert!(pump.buffer().is_empty());
    }
}

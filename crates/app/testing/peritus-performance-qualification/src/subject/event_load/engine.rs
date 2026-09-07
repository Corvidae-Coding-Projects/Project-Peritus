//! One producer owns the clock, queue, worker assignments, and accounting.

use std::collections::{BTreeMap, VecDeque};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::thread;
use std::time::{Duration, Instant};

use peritus_benchmarks::{
    AccountingSink, PlannedOperation, QualificationPlan, QueueKind, ResourceEvent,
};

use super::evidence::{EvidenceWriter, Observation, Summary};
use super::worker::{Completion, Executor, Job};
use crate::{CancellationFlag, SubjectError, effects::micros};

const POLL: Duration = Duration::from_millis(1);

pub struct Sinks<'a> {
    pub accounting: &'a mut dyn AccountingSink,
    pub evidence: &'a mut EvidenceWriter,
    pub on_completion: &'a mut dyn FnMut(&Completion) -> Result<(), SubjectError>,
}

pub fn run<W: Executor>(
    plan: &QualificationPlan,
    cancellation: &CancellationFlag,
    workers: Vec<W>,
    sinks: Sinks<'_>,
) -> Result<Summary, SubjectError> {
    thread::scope(|scope| {
        let (completed, results) = mpsc::sync_channel(workers.len());
        let mut senders = Vec::with_capacity(workers.len());
        let mut handles = Vec::with_capacity(workers.len());
        for (index, mut worker) in workers.into_iter().enumerate() {
            let (sender, receiver) = mpsc::sync_channel::<Job>(1);
            let completed = completed.clone();
            handles.push(thread::Builder::new().name(format!("h3-event-{index}")).spawn_scoped(
                scope,
                move || {
                    while let Ok(job) = receiver.recv() {
                        // Never reuse a panicked worker or leave its coordinator awaiting a reply.
                        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            worker.execute(index, &job)
                        }))
                        .map_err(|_| SubjectError::EventWorker("worker panicked"));
                        let failed = result.is_err();
                        if completed.send(result).is_err() || failed {
                            break;
                        }
                    }
                },
            )?);
            senders.push(sender);
        }
        drop(completed);
        let mut controller = Controller {
            plan,
            cancellation,
            origin: Instant::now(),
            idle: (0..senders.len()).collect(),
            senders: &senders,
            results,
            pending: VecDeque::new(),
            active: BTreeMap::new(),
            sinks,
            summary: Summary { expected: plan.step_count(), ..Summary::default() },
        };
        let result = controller.drive();
        // Drop the receiver before joining so an evidence/sink error cannot strand worker sends.
        drop(controller);
        drop(senders);
        let mut panicked = false;
        for handle in handles {
            panicked |= handle.join().is_err();
        }
        if panicked { Err(SubjectError::EventWorker("worker panicked")) } else { result }
    })
}

struct Controller<'a, 's> {
    plan: &'a QualificationPlan,
    cancellation: &'a CancellationFlag,
    origin: Instant,
    idle: Vec<usize>,
    senders: &'a [SyncSender<Job>],
    results: Receiver<Result<Completion, SubjectError>>,
    pending: VecDeque<Job>,
    active: BTreeMap<usize, u64>,
    sinks: Sinks<'s>,
    summary: Summary,
}

impl Controller<'_, '_> {
    fn drive(&mut self) -> Result<Summary, SubjectError> {
        for step in self.plan {
            self.wait_until(Duration::from_micros(step.offset_micros()))?;
            if self.cancellation.is_cancelled() {
                break;
            }
            let PlannedOperation::AppendEvent { bytes } = step.operation() else {
                return Err(SubjectError::Configuration(
                    "event plan contains a non-event operation".to_owned(),
                ));
            };
            let observed = micros(self.origin.elapsed());
            let next = self
                .plan
                .step(step.sequence() + 1)
                .map_or_else(|| micros(self.horizon()), |next| next.offset_micros());
            let outcome = if observed >= next {
                self.summary.schedule_missed += 1;
                "schedule_missed"
            } else {
                self.summary.offered += 1;
                self.offer(Job {
                    sequence: step.sequence(),
                    scheduled_us: step.offset_micros(),
                    payload_bytes: *bytes,
                    seed: self.plan.workload().parameters().seed(),
                    origin: self.origin,
                    horizon_us: micros(self.horizon()),
                })?
            };
            self.sinks.evidence.record(&Observation::Arrival {
                sequence: step.sequence(),
                scheduled_us: step.offset_micros(),
                observed_us: observed,
                payload_bytes: *bytes,
                outcome,
            })?;
        }
        self.wait_until(self.horizon())?;
        while let Some(job) = self.pending.pop_front() {
            self.pop_queue()?;
            self.summary.expired += 1;
            self.sinks.evidence.record(&Observation::Expired {
                sequence: job.sequence,
                observed_us: micros(self.origin.elapsed()),
            })?;
        }
        while !self.active.is_empty() {
            let completion = self
                .results
                .recv()
                .map_err(|_| SubjectError::EventWorker("active worker disconnected"))??;
            self.complete(&completion)?;
        }
        self.summary.elapsed_us = micros(self.origin.elapsed());
        Ok(std::mem::take(&mut self.summary))
    }

    const fn horizon(&self) -> Duration {
        Duration::from_secs(self.plan.workload().parameters().duration_seconds())
    }

    fn wait_until(&mut self, target: Duration) -> Result<(), SubjectError> {
        loop {
            while let Ok(completion) = self.results.try_recv() {
                self.complete(&completion?)?;
            }
            if self.cancellation.is_cancelled() {
                return Ok(());
            }
            while !self.idle.is_empty()
                && !self.pending.is_empty()
                && self.origin.elapsed() < self.horizon()
            {
                let job = self.pending.pop_front().expect("nonempty pending queue");
                self.pop_queue()?;
                self.dispatch(job)?;
            }
            let Some(remaining) = target.checked_sub(self.origin.elapsed()) else {
                return Ok(());
            };
            thread::sleep(remaining.min(POLL));
        }
    }

    fn offer(&mut self, job: Job) -> Result<&'static str, SubjectError> {
        if !self.idle.is_empty() && self.pending.is_empty() {
            self.dispatch(job)?;
            Ok("dispatched")
        } else if self.pending.len() < self.plan.workload().parameters().queue_capacity() as usize {
            self.sinks
                .accounting
                .apply(ResourceEvent::QueuePushed { queue: QueueKind::Command, count: 1 })?;
            self.pending.push_back(job);
            self.summary.peak_queued = self.summary.peak_queued.max(self.pending.len());
            Ok("queued")
        } else {
            self.summary.queue_full += 1;
            Ok("queue_full")
        }
    }

    fn dispatch(&mut self, job: Job) -> Result<(), SubjectError> {
        let index = self.idle.pop().expect("dispatch requires an idle worker");
        let parameters = self.plan.workload().parameters();
        self.sinks.accounting.apply(ResourceEvent::RunStarted {
            run: job.sequence,
            memory_bytes: parameters.memory_reservation_bytes(),
            disk_bytes: parameters.disk_reservation_bytes(),
            tokens: parameters.token_reservation(),
        })?;
        self.active.insert(index, job.sequence);
        self.summary.peak_active = self.summary.peak_active.max(self.active.len());
        self.senders[index]
            .send(job)
            .map_err(|_| SubjectError::EventWorker("idle worker disconnected"))
    }

    fn complete(&mut self, completion: &Completion) -> Result<(), SubjectError> {
        if self.active.remove(&completion.worker) != Some(completion.sequence) {
            return Err(SubjectError::EventWorker("completion does not match owned operation"));
        }
        self.idle.push(completion.worker);
        self.sinks.accounting.apply(ResourceEvent::RunFinished { run: completion.sequence })?;
        self.summary.committed_commands += u64::from(completion.committed_commands);
        if completion.failure.is_none() {
            self.summary.committed += 1;
        } else {
            self.summary.failed += 1;
        }
        self.sinks.evidence.record(&Observation::Completion(completion))?;
        (self.sinks.on_completion)(completion)
    }

    fn pop_queue(&mut self) -> Result<(), SubjectError> {
        self.sinks
            .accounting
            .apply(ResourceEvent::QueuePopped { queue: QueueKind::Command, count: 1 })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;

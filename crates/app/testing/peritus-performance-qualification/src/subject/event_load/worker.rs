//! A bounded owned A3 connection. One workload operation contains three real commits.

use peritus_types::RevisionTuple;
use serde::Serialize;
use std::time::Instant;

use crate::{
    SubjectError, a3::A3Client, effects::micros, event_payload::EventOperation,
    identity::IdentitySource,
};

#[derive(Clone)]
pub struct Job {
    pub sequence: u64,
    pub scheduled_us: u64,
    pub payload_bytes: u32,
    pub seed: u64,
    pub origin: Instant,
    pub horizon_us: u64,
}

pub struct Worker {
    pub client: A3Client,
    pub identities: IdentitySource,
    pub revision: RevisionTuple,
}

#[derive(Serialize)]
pub struct Completion {
    pub worker: usize,
    pub sequence: u64,
    pub scheduled_us: u64,
    pub started_us: u64,
    pub finished_us: u64,
    pub payload_bytes: u32,
    pub event_id: Option<[u8; 16]>,
    pub expected_event_digest: Option<peritus_benchmarks::Sha256Digest>,
    pub committed_commands: u32,
    pub failure: Option<String>,
}

impl Worker {
    fn submit(&mut self, job: &Job, completion: &mut Completion) -> Result<(), SubjectError> {
        let operation = EventOperation::prepare(
            &mut self.identities,
            self.revision,
            job.payload_bytes,
            job.seed ^ job.sequence,
        )?;
        completion.event_id = Some(operation.event_id());
        completion.expected_event_digest = Some(operation.event_digest());
        operation.submit(&mut self.client, &mut self.identities, &mut completion.committed_commands)
    }
}

pub trait Executor: Send {
    fn execute(&mut self, index: usize, job: &Job) -> Completion;
}

impl Executor for Worker {
    fn execute(&mut self, index: usize, job: &Job) -> Completion {
        let mut completion = Completion {
            worker: index,
            sequence: job.sequence,
            scheduled_us: job.scheduled_us,
            started_us: micros(job.origin.elapsed()),
            finished_us: 0,
            payload_bytes: job.payload_bytes,
            event_id: None,
            expected_event_digest: None,
            committed_commands: 0,
            failure: None,
        };
        if completion.started_us >= job.horizon_us {
            completion.finished_us = completion.started_us;
            completion.failure = Some("operation reached worker after load horizon".to_owned());
            return completion;
        }
        let result = self.submit(job, &mut completion);
        completion.finished_us = micros(job.origin.elapsed());
        completion.failure = result.err().map(|error| {
            let mut text = error.to_string();
            let mut cause = std::error::Error::source(&error);
            while let Some(source) = cause {
                use std::fmt::Write as _;
                write!(text, ": {source}").expect("writing to String");
                cause = source.source();
            }
            text
        });
        completion
    }
}

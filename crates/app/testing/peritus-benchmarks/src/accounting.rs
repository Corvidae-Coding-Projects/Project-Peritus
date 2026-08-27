//! Exact bounded resource and backpressure accounting for qualification runners.

use std::collections::{BTreeMap, BTreeSet};

use crate::{AccountingSummary, QualificationError, QueueKind, ResourceEnvelope, ResourceEvent};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RunReservation {
    memory_bytes: u64,
    disk_bytes: u64,
    tokens: u64,
}

/// Fail-closed ledger for runner-observed resources.
pub struct ResourceAccountant {
    envelope: ResourceEnvelope,
    runs: BTreeMap<u64, RunReservation>,
    processes: BTreeMap<u64, u64>,
    provider_requests: BTreeSet<u64>,
    memory_bytes: u64,
    disk_bytes: u64,
    tokens: u64,
    queues: [u32; 4],
    peak_runs: u32,
    peak_processes: u32,
    peak_provider_requests: u32,
    peak_memory_bytes: u64,
    peak_disk_bytes: u64,
    peak_tokens: u64,
    peak_queues: [u32; 4],
    backpressure_micros: [u64; 4],
    saturation_observations: u64,
}

impl ResourceAccountant {
    /// Starts an empty ledger bound to one profile envelope.
    #[must_use]
    pub const fn new(envelope: ResourceEnvelope) -> Self {
        Self {
            envelope,
            runs: BTreeMap::new(),
            processes: BTreeMap::new(),
            provider_requests: BTreeSet::new(),
            memory_bytes: 0,
            disk_bytes: 0,
            tokens: 0,
            queues: [0; 4],
            peak_runs: 0,
            peak_processes: 0,
            peak_provider_requests: 0,
            peak_memory_bytes: 0,
            peak_disk_bytes: 0,
            peak_tokens: 0,
            peak_queues: [0; 4],
            backpressure_micros: [0; 4],
            saturation_observations: 0,
        }
    }

    /// Applies one observation atomically, rejecting duplicate ownership and every bound bypass.
    ///
    /// # Errors
    ///
    /// Returns [`QualificationError`] when an event violates resource bounds, duplicates or
    /// mismatches a lifecycle transition, underflows a queue, or overflows checked accounting.
    pub fn apply(&mut self, event: ResourceEvent) -> Result<(), QualificationError> {
        match event {
            ResourceEvent::RunStarted { run, memory_bytes, disk_bytes, tokens } => {
                self.start_run(run, memory_bytes, disk_bytes, tokens)
            }
            ResourceEvent::RunFinished { run } => self.finish_run(run),
            ResourceEvent::ProcessStarted { process, memory_bytes } => {
                self.start_process(process, memory_bytes)
            }
            ResourceEvent::ProcessFinished { process } => self.finish_process(process),
            ResourceEvent::ProviderRequestStarted { request } => self.start_provider(request),
            ResourceEvent::ProviderRequestFinished { request } => self.finish_provider(request),
            ResourceEvent::QueuePushed { queue, count } => self.push_queue(queue, count),
            ResourceEvent::QueuePopped { queue, count } => self.pop_queue(queue, count),
            ResourceEvent::BackpressureObserved { queue, wait_micros } => {
                self.observe_backpressure(queue, wait_micros)
            }
            ResourceEvent::DiskRetained { bytes } => self.retain_disk(bytes),
            ResourceEvent::DiskReleased { bytes } => self.release_disk(bytes),
            ResourceEvent::TokensConsumed { tokens } => self.consume_tokens(tokens),
        }
    }

    /// Captures high-water values and terminal lifecycle balance.
    #[must_use]
    pub fn summary(&self) -> AccountingSummary {
        AccountingSummary {
            peak_runs: self.peak_runs,
            peak_processes: self.peak_processes,
            peak_provider_requests: self.peak_provider_requests,
            peak_memory_bytes: self.peak_memory_bytes,
            peak_disk_bytes: self.peak_disk_bytes,
            peak_tokens: self.peak_tokens,
            peak_command_queue: self.peak_queues[queue_index(QueueKind::Command)],
            peak_terminal_queue: self.peak_queues[queue_index(QueueKind::Terminal)],
            peak_exporter_queue: self.peak_queues[queue_index(QueueKind::Exporter)],
            peak_provider_queue: self.peak_queues[queue_index(QueueKind::Provider)],
            command_backpressure_micros: self.backpressure_micros[queue_index(QueueKind::Command)],
            terminal_backpressure_micros: self.backpressure_micros
                [queue_index(QueueKind::Terminal)],
            exporter_backpressure_micros: self.backpressure_micros
                [queue_index(QueueKind::Exporter)],
            provider_backpressure_micros: self.backpressure_micros
                [queue_index(QueueKind::Provider)],
            saturation_observations: self.saturation_observations,
            outstanding_runs: self.runs.len(),
            outstanding_processes: self.processes.len(),
            outstanding_provider_requests: self.provider_requests.len(),
            nonempty_queues: self.queues.iter().filter(|depth| **depth != 0).count(),
            retained_memory_bytes: self.memory_bytes,
            retained_disk_bytes: self.disk_bytes,
            retained_tokens: self.tokens,
        }
    }

    fn start_run(
        &mut self,
        run: u64,
        memory_bytes: u64,
        disk_bytes: u64,
        tokens: u64,
    ) -> Result<(), QualificationError> {
        if self.runs.contains_key(&run) {
            return Err(violation("run", "run was started more than once"));
        }
        let next_runs = add_count(self.runs.len(), 1, "active runs")?;
        require_u32(next_runs, self.envelope.concurrency().runs(), "active runs")?;
        let next_memory = checked_add(self.memory_bytes, memory_bytes, "memory")?;
        let next_disk = checked_add(self.disk_bytes, disk_bytes, "disk")?;
        let next_tokens = checked_add(self.tokens, tokens, "tokens")?;
        self.require_capacities(next_memory, next_disk, next_tokens)?;
        self.runs.insert(run, RunReservation { memory_bytes, disk_bytes, tokens });
        self.memory_bytes = next_memory;
        self.disk_bytes = next_disk;
        self.tokens = next_tokens;
        self.update_peaks();
        Ok(())
    }

    fn finish_run(&mut self, run: u64) -> Result<(), QualificationError> {
        let reservation =
            self.runs.remove(&run).ok_or_else(|| violation("run", "run was not active"))?;
        self.memory_bytes = self
            .memory_bytes
            .checked_sub(reservation.memory_bytes)
            .ok_or(QualificationError::ArithmeticOverflow("run memory release"))?;
        self.disk_bytes = self
            .disk_bytes
            .checked_sub(reservation.disk_bytes)
            .ok_or(QualificationError::ArithmeticOverflow("run disk release"))?;
        self.tokens = self
            .tokens
            .checked_sub(reservation.tokens)
            .ok_or(QualificationError::ArithmeticOverflow("run token release"))?;
        Ok(())
    }

    fn start_process(&mut self, process: u64, memory_bytes: u64) -> Result<(), QualificationError> {
        if self.processes.contains_key(&process) {
            return Err(violation("process", "process was started more than once"));
        }
        let next = add_count(self.processes.len(), 1, "active processes")?;
        require_u32(next, self.envelope.concurrency().processes(), "active processes")?;
        let next_memory = checked_add(self.memory_bytes, memory_bytes, "memory")?;
        self.require_capacities(next_memory, self.disk_bytes, self.tokens)?;
        self.processes.insert(process, memory_bytes);
        self.memory_bytes = next_memory;
        self.update_peaks();
        Ok(())
    }

    fn finish_process(&mut self, process: u64) -> Result<(), QualificationError> {
        let memory = self
            .processes
            .remove(&process)
            .ok_or_else(|| violation("process", "process was not active"))?;
        self.memory_bytes = self
            .memory_bytes
            .checked_sub(memory)
            .ok_or(QualificationError::ArithmeticOverflow("process memory release"))?;
        Ok(())
    }

    fn start_provider(&mut self, request: u64) -> Result<(), QualificationError> {
        if self.provider_requests.contains(&request) {
            return Err(violation("provider request", "request was started more than once"));
        }
        let next = add_count(self.provider_requests.len(), 1, "provider requests")?;
        require_u32(next, self.envelope.concurrency().provider_requests(), "provider requests")?;
        self.provider_requests.insert(request);
        self.update_peaks();
        Ok(())
    }

    fn finish_provider(&mut self, request: u64) -> Result<(), QualificationError> {
        if !self.provider_requests.remove(&request) {
            return Err(violation("provider request", "request was not active"));
        }
        Ok(())
    }

    fn push_queue(&mut self, queue: QueueKind, count: u32) -> Result<(), QualificationError> {
        if count == 0 {
            return Err(violation("queue", "push count must be greater than zero"));
        }
        let index = queue_index(queue);
        let next = self.queues[index]
            .checked_add(count)
            .ok_or(QualificationError::ArithmeticOverflow("queue depth"))?;
        if next > queue_limit(self.envelope, queue) {
            return Err(violation("queue", "push exceeds the declared queue capacity"));
        }
        self.queues[index] = next;
        self.peak_queues[index] = self.peak_queues[index].max(next);
        Ok(())
    }

    fn pop_queue(&mut self, queue: QueueKind, count: u32) -> Result<(), QualificationError> {
        if count == 0 {
            return Err(violation("queue", "pop count must be greater than zero"));
        }
        let index = queue_index(queue);
        self.queues[index] = self.queues[index]
            .checked_sub(count)
            .ok_or_else(|| violation("queue", "pop exceeds the observed queue depth"))?;
        Ok(())
    }

    fn observe_backpressure(
        &mut self,
        queue: QueueKind,
        wait_micros: u64,
    ) -> Result<(), QualificationError> {
        let index = queue_index(queue);
        if self.queues[index] != queue_limit(self.envelope, queue) {
            return Err(violation(
                "backpressure",
                "backpressure was reported while the queue was not full",
            ));
        }
        self.backpressure_micros[index] =
            checked_add(self.backpressure_micros[index], wait_micros, "backpressure wait")?;
        self.saturation_observations = self
            .saturation_observations
            .checked_add(1)
            .ok_or(QualificationError::ArithmeticOverflow("saturation observations"))?;
        Ok(())
    }

    fn retain_disk(&mut self, bytes: u64) -> Result<(), QualificationError> {
        let next = checked_add(self.disk_bytes, bytes, "disk")?;
        self.require_capacities(self.memory_bytes, next, self.tokens)?;
        self.disk_bytes = next;
        self.update_peaks();
        Ok(())
    }

    fn release_disk(&mut self, bytes: u64) -> Result<(), QualificationError> {
        self.disk_bytes = self
            .disk_bytes
            .checked_sub(bytes)
            .ok_or_else(|| violation("disk", "release exceeds retained bytes"))?;
        Ok(())
    }

    fn consume_tokens(&mut self, tokens: u64) -> Result<(), QualificationError> {
        let next = checked_add(self.tokens, tokens, "tokens")?;
        self.require_capacities(self.memory_bytes, self.disk_bytes, next)?;
        self.tokens = next;
        self.update_peaks();
        Ok(())
    }

    const fn require_capacities(
        &self,
        memory: u64,
        disk: u64,
        tokens: u64,
    ) -> Result<(), QualificationError> {
        let limits = self.envelope.capacity();
        if memory > limits.memory_bytes() {
            return Err(violation("memory", "reservation exceeds the memory envelope"));
        }
        if disk > limits.disk_bytes() {
            return Err(violation("disk", "reservation exceeds the disk envelope"));
        }
        if tokens > limits.tokens() {
            return Err(violation("tokens", "reservation exceeds the token envelope"));
        }
        Ok(())
    }

    fn update_peaks(&mut self) {
        self.peak_runs = self.peak_runs.max(u32::try_from(self.runs.len()).unwrap_or(u32::MAX));
        self.peak_processes =
            self.peak_processes.max(u32::try_from(self.processes.len()).unwrap_or(u32::MAX));
        self.peak_provider_requests = self
            .peak_provider_requests
            .max(u32::try_from(self.provider_requests.len()).unwrap_or(u32::MAX));
        self.peak_memory_bytes = self.peak_memory_bytes.max(self.memory_bytes);
        self.peak_disk_bytes = self.peak_disk_bytes.max(self.disk_bytes);
        self.peak_tokens = self.peak_tokens.max(self.tokens);
    }
}

const fn queue_index(queue: QueueKind) -> usize {
    match queue {
        QueueKind::Command => 0,
        QueueKind::Terminal => 1,
        QueueKind::Exporter => 2,
        QueueKind::Provider => 3,
    }
}

const fn queue_limit(envelope: ResourceEnvelope, queue: QueueKind) -> u32 {
    match queue {
        QueueKind::Command => envelope.queues().command(),
        QueueKind::Terminal => envelope.queues().terminal(),
        QueueKind::Exporter => envelope.queues().exporter(),
        QueueKind::Provider => envelope.queues().provider(),
    }
}

const fn checked_add(
    left: u64,
    right: u64,
    resource: &'static str,
) -> Result<u64, QualificationError> {
    match left.checked_add(right) {
        Some(value) => Ok(value),
        None => Err(QualificationError::ArithmeticOverflow(resource)),
    }
}

fn add_count(left: usize, right: usize, resource: &'static str) -> Result<u32, QualificationError> {
    let sum = left.checked_add(right).ok_or(QualificationError::ArithmeticOverflow(resource))?;
    u32::try_from(sum).map_err(|_| QualificationError::ArithmeticOverflow(resource))
}

const fn require_u32(
    value: u32,
    limit: u32,
    resource: &'static str,
) -> Result<(), QualificationError> {
    if value <= limit {
        Ok(())
    } else {
        Err(violation(resource, "count exceeds the declared envelope"))
    }
}

const fn violation(resource: &'static str, reason: &'static str) -> QualificationError {
    QualificationError::ResourceViolation { resource, reason }
}

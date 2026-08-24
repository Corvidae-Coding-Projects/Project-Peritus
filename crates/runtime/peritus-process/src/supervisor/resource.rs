//! Local resource support admission, sampling, and ceiling enforcement.

use std::{path::Path, time::Instant};

use crate::{
    BackendResourceFidelity, ErrorCode, ExecutionIsolation, ExecutionPlan, ProcessError,
    ProcessEventKind, ProcessOperation, ProcessResourceDimension, ProcessResourceObservation,
    RecoveryClass, ResourceFidelity,
    control::SharedObservation,
    platform::{self, ProcessTreeIdentity},
};

use super::{elapsed_millis, emit};

const SAMPLE_INTERVAL_MILLIS: u64 = 20;
const DISK_SAMPLE_INTERVAL_MILLIS: u64 = 1_000;

pub(crate) fn validate_launch(plan: &ExecutionPlan) -> Result<(), ProcessError> {
    if plan.isolation() != ExecutionIsolation::ExplicitRawEffect {
        return Err(ProcessError::new(
            ErrorCode::Unsupported,
            ProcessOperation::Validate,
            RecoveryClass::SelectBackend,
            "local execution requires explicit raw-effect authority",
        ));
    }
    if plan.backend().resource_fidelity() != BackendResourceFidelity::Reference
        && !platform::local_supervisor_resources_supported()
    {
        return Err(ProcessError::new(
            ErrorCode::Unsupported,
            ProcessOperation::Validate,
            RecoveryClass::SelectBackend,
            "the selected backend requires unavailable local resource enforcement",
        ));
    }
    Ok(())
}

pub(crate) fn validate_native_launch(plan: &ExecutionPlan) -> Result<(), ProcessError> {
    if plan.isolation() != ExecutionIsolation::Restricted
        || plan.backend().resource_fidelity() == BackendResourceFidelity::Reference
    {
        return Err(ProcessError::new(
            ErrorCode::Unsupported,
            ProcessOperation::Validate,
            RecoveryClass::SelectBackend,
            "native execution requires restricted authority and a non-reference resource enforcer",
        ));
    }
    Ok(())
}

pub(super) struct ResourceTracker {
    sampling_supported: bool,
    baseline_disk: u64,
    greatest_cpu: u64,
    greatest_memory: u64,
    greatest_disk: u64,
    greatest_processes: u64,
    greatest_handles: u64,
    last_sample: Option<Instant>,
    last_disk_sample: Option<Instant>,
}

impl ResourceTracker {
    pub(super) fn start(plan: &ExecutionPlan) -> Result<Self, ProcessError> {
        let sampling_supported = platform::local_supervisor_resources_supported();
        Ok(Self {
            sampling_supported,
            baseline_disk: if sampling_supported {
                disk_usage(plan.working_directory().path())?
            } else {
                0
            },
            greatest_cpu: 0,
            greatest_memory: 0,
            greatest_disk: 0,
            greatest_processes: 1,
            greatest_handles: 0,
            last_sample: None,
            last_disk_sample: Some(Instant::now()),
        })
    }

    pub(super) fn sample(
        &mut self,
        tree: ProcessTreeIdentity,
        plan: &ExecutionPlan,
        shared: &std::sync::Arc<SharedObservation>,
        force: bool,
    ) -> Result<bool, ProcessError> {
        if !self.sampling_supported {
            return Ok(false);
        }
        if !force
            && self
                .last_sample
                .is_some_and(|sample| elapsed_millis(sample) < SAMPLE_INTERVAL_MILLIS)
        {
            return Ok(self.exceeded(plan));
        }
        let sample = platform::sample_resources(tree)?;
        let sample_disk = force
            || self
                .last_disk_sample
                .is_none_or(|sample| elapsed_millis(sample) >= DISK_SAMPLE_INTERVAL_MILLIS);
        if sample_disk {
            let disk =
                disk_usage(plan.working_directory().path())?.saturating_sub(self.baseline_disk);
            self.greatest_disk = self.greatest_disk.max(disk);
            self.last_disk_sample = Some(Instant::now());
        }
        self.greatest_cpu = self.greatest_cpu.max(sample.cpu_millis());
        self.greatest_memory = self.greatest_memory.max(sample.memory_bytes());
        self.greatest_processes = self.greatest_processes.max(sample.process_count());
        self.greatest_handles = self.greatest_handles.max(sample.open_handles());
        self.last_sample = Some(Instant::now());
        emit(shared, plan, None, ProcessEventKind::ResourceSample, Vec::new());
        Ok(self.exceeded(plan))
    }

    pub(super) fn observations(
        &self,
        plan: &ExecutionPlan,
        began: Instant,
        output: u64,
    ) -> Vec<ProcessResourceObservation> {
        let ceiling = plan.resource_policy();
        vec![
            observation(
                ProcessResourceDimension::WallTimeMilliseconds,
                elapsed_millis(began),
                ceiling.wall_millis(),
                ResourceFidelity::Enforced,
            ),
            observation(
                ProcessResourceDimension::CpuTimeMilliseconds,
                self.greatest_cpu,
                ceiling.cpu_millis(),
                self.sampled_fidelity(),
            ),
            observation(
                ProcessResourceDimension::MemoryBytes,
                self.greatest_memory,
                ceiling.memory_bytes(),
                self.sampled_fidelity(),
            ),
            observation(
                ProcessResourceDimension::DiskBytes,
                self.greatest_disk,
                ceiling.disk_bytes(),
                self.sampled_fidelity(),
            ),
            observation(
                ProcessResourceDimension::OutputBytes,
                output,
                ceiling.output_bytes(),
                ResourceFidelity::Enforced,
            ),
            observation(
                ProcessResourceDimension::ProcessCount,
                self.greatest_processes,
                ceiling.process_count(),
                self.sampled_fidelity(),
            ),
            observation(
                ProcessResourceDimension::OpenHandles,
                self.greatest_handles,
                ceiling.file_descriptors(),
                self.sampled_fidelity(),
            ),
            observation(
                ProcessResourceDimension::ConcurrencySlots,
                1,
                ceiling.concurrent_slots(),
                ResourceFidelity::Enforced,
            ),
        ]
    }

    pub(super) fn observe_process_count(&mut self, process_count: u64) {
        self.greatest_processes = self.greatest_processes.max(process_count);
    }

    pub(super) const fn requires_process_count_sample(&self) -> bool {
        !self.sampling_supported
    }

    pub(super) const fn limit_exceeded(&self, plan: &ExecutionPlan) -> bool {
        self.exceeded(plan)
    }

    const fn exceeded(&self, plan: &ExecutionPlan) -> bool {
        let ceiling = plan.resource_policy();
        self.greatest_cpu > ceiling.cpu_millis()
            || self.greatest_memory > ceiling.memory_bytes()
            || self.greatest_disk > ceiling.disk_bytes()
            || self.greatest_processes > ceiling.process_count()
            || self.greatest_handles > ceiling.file_descriptors()
    }

    const fn sampled_fidelity(&self) -> ResourceFidelity {
        if self.sampling_supported {
            ResourceFidelity::Sampled
        } else {
            ResourceFidelity::Unsupported
        }
    }
}

const fn observation(
    dimension: ProcessResourceDimension,
    value: u64,
    ceiling: u64,
    fidelity: ResourceFidelity,
) -> ProcessResourceObservation {
    ProcessResourceObservation::new(dimension, value, ceiling, fidelity)
}

fn disk_usage(root: &Path) -> Result<u64, ProcessError> {
    let mut pending = vec![root.to_path_buf()];
    let mut total = 0_u64;
    while let Some(directory) = pending.pop() {
        let entries = std::fs::read_dir(directory)
            .map_err(|_| resource_error("workspace disk usage cannot be observed"))?;
        for entry in entries {
            let entry = entry.map_err(|_| resource_error("workspace entry cannot be observed"))?;
            let metadata = std::fs::symlink_metadata(entry.path())
                .map_err(|_| resource_error("workspace metadata cannot be observed"))?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                total = total.saturating_add(metadata.len());
            }
        }
    }
    Ok(total)
}

const fn resource_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::ResourceLimit,
        ProcessOperation::Wait,
        RecoveryClass::CancelAndReap,
        detail,
    )
}

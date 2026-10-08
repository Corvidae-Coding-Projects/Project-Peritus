//! Local resource support admission, sampling, and ceiling enforcement.

use std::time::Instant;

use crate::{
    BackendResourceFidelity, ErrorCode, ExecutionIsolation, ExecutionPlan, ProcessError,
    ProcessEventKind, ProcessOperation, ProcessResourceDimension, ProcessResourceObservation,
    RecoveryClass, ResourceFidelity,
    control::SharedObservation,
    platform::{self, ProcessTreeIdentity},
};

use super::{SupervisorPlan, elapsed_millis, emit};

mod disk;
mod sampler;

use sampler::{ResourceSampler, SamplerUpdate};

#[derive(Clone, Copy)]
enum UnknownMeasurementRecovery {
    ContinueWithIncompleteEvidence,
}

const UNKNOWN_MEASUREMENT_RECOVERY: UnknownMeasurementRecovery =
    UnknownMeasurementRecovery::ContinueWithIncompleteEvidence;

#[derive(Clone, Copy)]
enum LimitObservation {
    Within,
    Exceeded,
    Unknown,
}

pub(crate) fn validate_launch(plan: &ExecutionPlan) -> Result<(), ProcessError> {
    platform::validate_native_admission(plan)?;
    if plan.isolation() != ExecutionIsolation::ExplicitRawEffect {
        return Err(ProcessError::new(
            ErrorCode::Unsupported,
            ProcessOperation::Validate,
            RecoveryClass::SelectBackend,
            "local execution requires explicit raw-effect authority",
        ));
    }
    validate_capabilities(plan, platform::local_execution_capabilities(plan.io_mode()))
}

pub(crate) fn validate_native_launch(
    plan: &ExecutionPlan,
    descriptor: &peritus_sandbox::BackendDescriptor,
) -> Result<(), ProcessError> {
    platform::validate_native_admission(plan)?;
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
    validate_capabilities(plan, platform::native_execution_capabilities(descriptor))
}

fn validate_capabilities(
    plan: &ExecutionPlan,
    capabilities: platform::PlatformExecutionCapabilities,
) -> Result<(), ProcessError> {
    if !capabilities.complete_tree_containment() {
        return Err(missing_capability(
            "the selected launch path cannot provide complete process-tree containment",
        ));
    }
    let resources = capabilities.resources();
    let policy = plan.resource_policy();
    for (selected, supported, detail) in [
        (
            policy.wall_millis().is_some(),
            resources.wall_time(),
            "the selected launch path cannot enforce the wall-time ceiling",
        ),
        (
            policy.cpu_millis().is_some(),
            resources.cpu_time(),
            "the selected launch path cannot enforce the CPU-time ceiling",
        ),
        (
            policy.memory_limit().is_some(),
            resources.memory(),
            "the selected launch path cannot enforce the memory ceiling",
        ),
        (
            policy.disk_limit().is_some(),
            resources.disk(),
            "the selected launch path cannot enforce the disk ceiling",
        ),
        (
            policy.output_limit().is_some(),
            resources.output(),
            "the selected launch path cannot enforce the output ceiling",
        ),
        (
            policy.process_limit().is_some(),
            resources.process_count(),
            "the selected launch path cannot enforce the process-count ceiling",
        ),
        (
            policy.file_descriptor_limit().is_some(),
            resources.open_handles(),
            "the selected launch path cannot enforce the open-handle ceiling",
        ),
        (
            true,
            resources.concurrency(),
            "the selected launch path cannot enforce the concurrency ceiling",
        ),
    ] {
        if selected && !supported {
            return Err(missing_capability(detail));
        }
    }
    Ok(())
}

const fn missing_capability(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Unsupported,
        ProcessOperation::Validate,
        RecoveryClass::SelectBackend,
        detail,
    )
}

pub(super) struct ResourceTracker {
    cpu: Metric,
    memory: Metric,
    disk: Metric,
    processes: Metric,
    handles: Metric,
    sampler: Option<ResourceSampler>,
}

impl ResourceTracker {
    #[allow(
        clippy::unnecessary_wraps,
        reason = "preserves the owner construction boundary while sampling startup becomes evidence"
    )]
    pub(super) fn start(plan: &SupervisorPlan) -> Result<Self, ProcessError> {
        let capabilities = platform::local_resource_capabilities();
        let sampler = if capabilities.has_sampler() {
            ResourceSampler::start(plan.working_directory().to_path_buf()).ok()
        } else {
            None
        };
        let mut tracker = Self {
            cpu: Metric::new(capabilities.cpu_time(), 0),
            memory: Metric::new(capabilities.memory(), 0),
            disk: Metric::new(capabilities.disk(), 0),
            processes: Metric::new(capabilities.process_count(), 1),
            handles: Metric::new(capabilities.open_handles(), 0),
            sampler,
        };
        if capabilities.has_sampler() && tracker.sampler.is_none() {
            tracker.mark_process_unavailable();
            tracker.disk.unavailable();
        }
        Ok(tracker)
    }

    pub(super) fn sample(
        &mut self,
        tree: ProcessTreeIdentity,
        plan: &SupervisorPlan,
        shared: &std::sync::Arc<SharedObservation>,
        _force: bool,
    ) -> bool {
        let update = self.sampler.as_ref().map(|sampler| {
            sampler.attach(tree);
            sampler.poll()
        });
        if update.is_some_and(|update| self.apply(update)) {
            emit(shared, plan, None, ProcessEventKind::ResourceSample, Vec::new());
        }
        self.exceeded(plan)
    }

    pub(super) fn attach(&self, tree: ProcessTreeIdentity) {
        if let Some(sampler) = &self.sampler {
            sampler.attach(tree);
        }
    }

    pub(super) fn request_finish(
        &mut self,
        tree: ProcessTreeIdentity,
        plan: &SupervisorPlan,
        shared: &std::sync::Arc<SharedObservation>,
    ) {
        let update = self.sampler.as_mut().map(|sampler| {
            sampler.attach(tree);
            sampler.request_finish()
        });
        if update.is_some_and(|update| self.apply(update)) {
            emit(shared, plan, None, ProcessEventKind::ResourceSample, Vec::new());
        }
    }

    pub(super) fn retire(
        &mut self,
        plan: &SupervisorPlan,
        shared: &std::sync::Arc<SharedObservation>,
    ) {
        let update = self.sampler.take().map(|mut sampler| sampler.retire());
        if update.is_some_and(|update| self.apply(update)) {
            emit(shared, plan, None, ProcessEventKind::ResourceSample, Vec::new());
        }
    }

    pub(super) fn observations(
        &self,
        plan: &SupervisorPlan,
        began: Instant,
        output: u64,
    ) -> Vec<ProcessResourceObservation> {
        let ceiling = plan.resource_policy();
        vec![
            observation(
                ProcessResourceDimension::WallTimeMilliseconds,
                elapsed_millis(began),
                ceiling.wall_millis(),
                if ceiling.wall_millis().is_some() {
                    ResourceFidelity::Enforced
                } else {
                    ResourceFidelity::Sampled
                },
            ),
            observation(
                ProcessResourceDimension::CpuTimeMilliseconds,
                self.cpu.greatest,
                ceiling.cpu_millis(),
                self.cpu.fidelity(),
            ),
            observation(
                ProcessResourceDimension::MemoryBytes,
                self.memory.greatest,
                ceiling.memory_limit(),
                self.memory.fidelity(),
            ),
            observation(
                ProcessResourceDimension::DiskBytes,
                self.disk.greatest,
                ceiling.disk_limit(),
                self.disk.fidelity(),
            ),
            observation(
                ProcessResourceDimension::OutputBytes,
                output,
                ceiling.output_limit(),
                if ceiling.output_limit().is_some() {
                    ResourceFidelity::Enforced
                } else {
                    ResourceFidelity::Sampled
                },
            ),
            observation(
                ProcessResourceDimension::ProcessCount,
                self.processes.greatest,
                ceiling.process_limit(),
                self.processes.fidelity(),
            ),
            observation(
                ProcessResourceDimension::OpenHandles,
                self.handles.greatest,
                ceiling.file_descriptor_limit(),
                self.handles.fidelity(),
            ),
            observation(
                ProcessResourceDimension::ConcurrencySlots,
                1,
                Some(ceiling.concurrent_slots()),
                ResourceFidelity::Enforced,
            ),
        ]
    }

    pub(super) fn limit_exceeded(&self, plan: &SupervisorPlan) -> bool {
        self.exceeded(plan)
    }

    pub(super) fn recover_unknown_native_measurement(
        &mut self,
        plan: &SupervisorPlan,
        error: &ProcessError,
    ) -> Option<bool> {
        if error.code() != ErrorCode::ResourceLimit
            || error.recovery() != RecoveryClass::CancelAndReap
        {
            return None;
        }
        let ceiling = plan.resource_policy();
        let changed = self.cpu.selected_unavailable(ceiling.cpu_millis().is_some())
            | self.memory.selected_unavailable(ceiling.memory_limit().is_some())
            | self.disk.selected_unavailable(ceiling.disk_limit().is_some())
            | self.processes.selected_unavailable(ceiling.process_limit().is_some())
            | self
                .handles
                .selected_unavailable(ceiling.file_descriptor_limit().is_some());
        match UNKNOWN_MEASUREMENT_RECOVERY {
            UnknownMeasurementRecovery::ContinueWithIncompleteEvidence => Some(changed),
        }
    }

    fn exceeded(&self, plan: &SupervisorPlan) -> bool {
        let ceiling = plan.resource_policy();
        ceiling.cpu_millis().is_some_and(|maximum| selected_exceeded(&self.cpu, maximum))
            || ceiling
                .memory_limit()
                .is_some_and(|maximum| selected_exceeded(&self.memory, maximum))
            || ceiling
                .disk_limit()
                .is_some_and(|maximum| selected_exceeded(&self.disk, maximum))
            || ceiling
                .process_limit()
                .is_some_and(|maximum| selected_exceeded(&self.processes, maximum))
            || ceiling
                .file_descriptor_limit()
                .is_some_and(|maximum| selected_exceeded(&self.handles, maximum))
    }

    fn apply(&mut self, update: SamplerUpdate) -> bool {
        if update.is_empty() {
            return false;
        }
        if let Some(process) = update.process {
            self.cpu.observe_sample(process.cpu_millis());
            self.memory.observe_sample(process.memory_bytes());
            self.processes.observe_sample(process.process_count());
            self.handles.observe_sample(process.open_handles());
        }
        if let Some(disk) = update.disk_growth {
            self.disk.observe_sample(disk);
        }
        true
    }

    fn mark_process_unavailable(&mut self) {
        self.cpu.unavailable();
        self.memory.unavailable();
        self.processes.unavailable();
        self.handles.unavailable();
    }
}

struct Metric {
    greatest: u64,
    proven_greatest: Option<u64>,
    supported: bool,
    observed: bool,
    incomplete: bool,
}

impl Metric {
    const fn new(supported: bool, initial: u64) -> Self {
        Self {
            greatest: initial,
            proven_greatest: None,
            supported,
            observed: false,
            incomplete: false,
        }
    }

    fn observe_sample(&mut self, sample: sampler::SampleValue) {
        if let Some(value) = sample.greatest() {
            self.supported = true;
            self.greatest = self.greatest.max(value);
            self.observed = true;
        }
        if let Some(value) = sample.proven_greatest() {
            self.proven_greatest =
                Some(self.proven_greatest.map_or(value, |current| current.max(value)));
        }
        self.incomplete |= !sample.available();
    }

    const fn unavailable(&mut self) {
        if self.supported {
            self.incomplete = true;
        }
    }

    const fn selected_unavailable(&mut self, selected: bool) -> bool {
        if !selected {
            return false;
        }
        let changed = !self.supported || !self.incomplete;
        self.supported = true;
        self.incomplete = true;
        changed
    }

    const fn limit_observation(&self, ceiling: u64) -> LimitObservation {
        if matches!(self.proven_greatest, Some(value) if value > ceiling) {
            LimitObservation::Exceeded
        } else if !self.supported || !self.observed || self.incomplete {
            LimitObservation::Unknown
        } else {
            LimitObservation::Within
        }
    }

    const fn fidelity(&self) -> ResourceFidelity {
        if !self.supported {
            ResourceFidelity::Unsupported
        } else if self.incomplete || !self.observed {
            ResourceFidelity::Incomplete
        } else {
            ResourceFidelity::Sampled
        }
    }
}

const fn selected_exceeded(metric: &Metric, ceiling: u64) -> bool {
    match metric.limit_observation(ceiling) {
        LimitObservation::Exceeded => true,
        LimitObservation::Within => false,
        LimitObservation::Unknown => match UNKNOWN_MEASUREMENT_RECOVERY {
            UnknownMeasurementRecovery::ContinueWithIncompleteEvidence => false,
        },
    }
}

const fn observation(
    dimension: ProcessResourceDimension,
    value: u64,
    ceiling: Option<u64>,
    fidelity: ResourceFidelity,
) -> ProcessResourceObservation {
    ProcessResourceObservation::with_optional_ceiling(dimension, value, ceiling, fidelity)
}

#[cfg(test)]
const fn resource_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::ResourceLimit,
        ProcessOperation::Wait,
        RecoveryClass::CancelAndReap,
        detail,
    )
}

//! Independently checked scheduler bounds.

use crate::{SchedulerError, SchedulerErrorKind};

/// One-based bounded work attempt number.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AttemptNumber(u16);

impl AttemptNumber {
    /// Creates a nonzero attempt number.
    ///
    /// # Errors
    /// Rejects zero.
    pub fn new(value: u16) -> Result<Self, SchedulerError> {
        if value == 0 {
            Err(crate::error::reject(SchedulerErrorKind::InvalidInput, "attempt number is zero"))
        } else {
            Ok(Self(value))
        }
    }
    /// Returns the one-based number.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
    pub(crate) const fn from_wire(value: u16) -> Self {
        Self(value)
    }
}

/// Complete independently enforced limits for one scheduler aggregate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SchedulerLimits {
    queued_work: u32,
    retained_work: u32,
    workers: u16,
    dependencies_per_work: u16,
    resource_dimensions: u16,
    active_reservations: u16,
    attempts_per_work: u16,
    bypass_count: u16,
    dispatch_batch_size: u16,
    payload_bytes: u64,
    state_bytes: u64,
}

impl SchedulerLimits {
    /// Compiled maximum queued items.
    pub const MAX_QUEUED_WORK: u32 = 65_535;
    /// Compiled maximum retained items.
    pub const MAX_RETAINED_WORK: u32 = 65_535;
    /// Compiled maximum workers.
    pub const MAX_WORKERS: u16 = 4_096;
    /// Compiled maximum dependencies per work item.
    pub const MAX_DEPENDENCIES: u16 = 1_024;
    /// Compiled maximum resource dimensions.
    pub const MAX_RESOURCE_DIMENSIONS: u16 = 256;
    /// Compiled maximum simultaneous reservations.
    pub const MAX_ACTIVE_RESERVATIONS: u16 = 4_096;
    /// Compiled maximum attempts per work item.
    pub const MAX_ATTEMPTS: u16 = 1_024;
    /// Compiled maximum deterministic bypass counter.
    pub const MAX_BYPASS_COUNT: u16 = 32_767;
    /// Compiled maximum dispatches requested by one effect-shell poll.
    pub const MAX_DISPATCH_BATCH: u16 = 256;
    /// Maximum canonical command/event payload bytes.
    pub const MAX_PAYLOAD_BYTES: u64 = 16 * 1_048_576 - 16;
    /// Maximum canonical complete checkpoint bytes.
    pub const MAX_STATE_BYTES: u64 = 16 * 1_048_576 - 16;

    /// Creates limits after validating every field independently.
    ///
    /// # Errors
    /// Rejects zero values, values above compiled ceilings, or a queue bound above retention.
    #[allow(clippy::too_many_arguments, reason = "independent scheduler bounds stay explicit")]
    pub fn new(
        queued_work: u32,
        retained_work: u32,
        workers: u16,
        dependencies_per_work: u16,
        resource_dimensions: u16,
        active_reservations: u16,
        attempts_per_work: u16,
        bypass_count: u16,
        dispatch_batch_size: u16,
        payload_bytes: u64,
        state_bytes: u64,
    ) -> Result<Self, SchedulerError> {
        let checks = [
            (u64::from(queued_work), u64::from(Self::MAX_QUEUED_WORK)),
            (u64::from(retained_work), u64::from(Self::MAX_RETAINED_WORK)),
            (u64::from(workers), u64::from(Self::MAX_WORKERS)),
            (u64::from(dependencies_per_work), u64::from(Self::MAX_DEPENDENCIES)),
            (u64::from(resource_dimensions), u64::from(Self::MAX_RESOURCE_DIMENSIONS)),
            (u64::from(active_reservations), u64::from(Self::MAX_ACTIVE_RESERVATIONS)),
            (u64::from(attempts_per_work), u64::from(Self::MAX_ATTEMPTS)),
            (u64::from(bypass_count), u64::from(Self::MAX_BYPASS_COUNT)),
            (u64::from(dispatch_batch_size), u64::from(Self::MAX_DISPATCH_BATCH)),
            (payload_bytes, Self::MAX_PAYLOAD_BYTES),
            (state_bytes, Self::MAX_STATE_BYTES),
        ];
        if checks.into_iter().any(|(value, maximum)| value == 0 || value > maximum)
            || queued_work > retained_work
            || u32::from(active_reservations) > retained_work
        {
            return Err(crate::error::reject(
                SchedulerErrorKind::InvalidLimit,
                "scheduler limit is zero, inconsistent, or above its production ceiling",
            ));
        }
        Ok(Self::from_wire(
            queued_work,
            retained_work,
            workers,
            dependencies_per_work,
            resource_dimensions,
            active_reservations,
            attempts_per_work,
            bypass_count,
            dispatch_batch_size,
            payload_bytes,
            state_bytes,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn from_wire(
        queued_work: u32,
        retained_work: u32,
        workers: u16,
        dependencies_per_work: u16,
        resource_dimensions: u16,
        active_reservations: u16,
        attempts_per_work: u16,
        bypass_count: u16,
        dispatch_batch_size: u16,
        payload_bytes: u64,
        state_bytes: u64,
    ) -> Self {
        Self {
            queued_work,
            retained_work,
            workers,
            dependencies_per_work,
            resource_dimensions,
            active_reservations,
            attempts_per_work,
            bypass_count,
            dispatch_batch_size,
            payload_bytes,
            state_bytes,
        }
    }

    /// Maximum simultaneously queued work.
    #[must_use]
    pub const fn queued_work(self) -> u32 {
        self.queued_work
    }
    /// Maximum retained work history.
    #[must_use]
    pub const fn retained_work(self) -> u32 {
        self.retained_work
    }
    /// Maximum retained workers.
    #[must_use]
    pub const fn workers(self) -> u16 {
        self.workers
    }
    /// Maximum dependencies on one work item.
    #[must_use]
    pub const fn dependencies_per_work(self) -> u16 {
        self.dependencies_per_work
    }
    /// Maximum resource dimensions in one vector.
    #[must_use]
    pub const fn resource_dimensions(self) -> u16 {
        self.resource_dimensions
    }
    /// Maximum live reservations.
    #[must_use]
    pub const fn active_reservations(self) -> u16 {
        self.active_reservations
    }
    /// Maximum attempts for one item.
    #[must_use]
    pub const fn attempts_per_work(self) -> u16 {
        self.attempts_per_work
    }
    /// Bypasses before a feasible item is forced ahead.
    #[must_use]
    pub const fn bypass_count(self) -> u16 {
        self.bypass_count
    }
    /// Maximum directives returned by one runtime poll.
    #[must_use]
    pub const fn dispatch_batch_size(self) -> u16 {
        self.dispatch_batch_size
    }
    /// Maximum command/event payload bytes.
    #[must_use]
    pub const fn payload_bytes(self) -> u64 {
        self.payload_bytes
    }
    /// Maximum checkpoint bytes.
    #[must_use]
    pub const fn state_bytes(self) -> u64 {
        self.state_bytes
    }

    pub(crate) fn validate(self) -> Result<(), SchedulerError> {
        Self::new(
            self.queued_work,
            self.retained_work,
            self.workers,
            self.dependencies_per_work,
            self.resource_dimensions,
            self.active_reservations,
            self.attempts_per_work,
            self.bypass_count,
            self.dispatch_batch_size,
            self.payload_bytes,
            self.state_bytes,
        )
        .map(|_| ())
    }
}

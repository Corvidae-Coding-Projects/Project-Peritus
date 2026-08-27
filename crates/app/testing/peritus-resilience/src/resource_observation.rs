//! Retry, resource, ownership, and cleanup accounting observations.

/// Reconciliation accounting for daemon-owned work and orphan candidates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OwnershipObservation {
    scan_completed: bool,
    discovered: u16,
    resumed: u16,
    failed: u16,
    indeterminate: u16,
    unaccounted: u16,
    orphan_candidates_detected: u16,
    orphans_remaining: u16,
}

/// Truthful terminal classifications for discovered owned work.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct OwnershipResolution {
    resumed: u16,
    failed: u16,
    indeterminate: u16,
    unaccounted: u16,
}

impl OwnershipResolution {
    /// Creates direct resolution counts.
    #[must_use]
    pub const fn new(resumed: u16, failed: u16, indeterminate: u16, unaccounted: u16) -> Self {
        Self { resumed, failed, indeterminate, unaccounted }
    }
}

impl OwnershipObservation {
    /// Creates direct bounded ownership counts.
    #[must_use]
    pub const fn new(
        scan_completed: bool,
        discovered: u16,
        resolution: OwnershipResolution,
        orphan_candidates_detected: u16,
        orphans_remaining: u16,
    ) -> Self {
        Self {
            scan_completed,
            discovered,
            resumed: resolution.resumed,
            failed: resolution.failed,
            indeterminate: resolution.indeterminate,
            unaccounted: resolution.unaccounted,
            orphan_candidates_detected,
            orphans_remaining,
        }
    }
    /// Returns whether the ownership/orphan scan completed.
    #[must_use]
    pub const fn scan_completed(self) -> bool {
        self.scan_completed
    }
    /// Returns the number of outstanding owned items discovered.
    #[must_use]
    pub const fn discovered(self) -> u16 {
        self.discovered
    }
    /// Returns the number resumed under explicit ownership.
    #[must_use]
    pub const fn resumed(self) -> u16 {
        self.resumed
    }
    /// Returns the number explicitly failed.
    #[must_use]
    pub const fn failed(self) -> u16 {
        self.failed
    }
    /// Returns the number explicitly marked indeterminate.
    #[must_use]
    pub const fn indeterminate(self) -> u16 {
        self.indeterminate
    }
    /// Returns the number not assigned a truthful outcome.
    #[must_use]
    pub const fn unaccounted(self) -> u16 {
        self.unaccounted
    }
    /// Returns potential orphans found and brought into explicit reconciliation.
    #[must_use]
    pub const fn orphan_candidates_detected(self) -> u16 {
        self.orphan_candidates_detected
    }
    /// Returns actual work remaining outside authoritative ownership.
    #[must_use]
    pub const fn orphans_remaining(self) -> u16 {
        self.orphans_remaining
    }
}

/// Governed retries/restarts consumed by one scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RetryUsage {
    provider: u16,
    tool: u16,
    worker: u16,
    reconciliation: u16,
}

impl RetryUsage {
    /// Creates direct retry counts.
    #[must_use]
    pub const fn new(provider: u16, tool: u16, worker: u16, reconciliation: u16) -> Self {
        Self { provider, tool, worker, reconciliation }
    }
    /// Returns provider retries.
    #[must_use]
    pub const fn provider(self) -> u16 {
        self.provider
    }
    /// Returns tool retries.
    #[must_use]
    pub const fn tool(self) -> u16 {
        self.tool
    }
    /// Returns worker restarts.
    #[must_use]
    pub const fn worker(self) -> u16 {
        self.worker
    }
    /// Returns reconciliation steps.
    #[must_use]
    pub const fn reconciliation(self) -> u16 {
        self.reconciliation
    }
}

/// Deterministic resource usage observed for one scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceUsage {
    events: u32,
    evidence_bytes: u32,
    peak_owned_processes: u16,
    cleanup_steps: u16,
    logical_ticks: u64,
}

impl ResourceUsage {
    /// Creates direct resource counters.
    #[must_use]
    pub const fn new(
        events: u32,
        evidence_bytes: u32,
        peak_owned_processes: u16,
        cleanup_steps: u16,
        logical_ticks: u64,
    ) -> Self {
        Self { events, evidence_bytes, peak_owned_processes, cleanup_steps, logical_ticks }
    }
    /// Returns emitted/retained event count.
    #[must_use]
    pub const fn events(self) -> u32 {
        self.events
    }
    /// Returns total retained evidence bytes.
    #[must_use]
    pub const fn evidence_bytes(self) -> u32 {
        self.evidence_bytes
    }
    /// Returns peak simultaneously owned process count.
    #[must_use]
    pub const fn peak_owned_processes(self) -> u16 {
        self.peak_owned_processes
    }
    /// Returns cleanup operation count.
    #[must_use]
    pub const fn cleanup_steps(self) -> u16 {
        self.cleanup_steps
    }
    /// Returns runtime-neutral deterministic time consumed.
    #[must_use]
    pub const fn logical_ticks(self) -> u64 {
        self.logical_ticks
    }
}

/// Cleanup proof returned while consuming a fresh subject.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CleanupObservation {
    resources_released: bool,
    owned_work_remaining: u16,
    cleanup_steps: u16,
}

impl CleanupObservation {
    /// Creates direct cleanup facts.
    #[must_use]
    pub const fn new(
        resources_released: bool,
        owned_work_remaining: u16,
        cleanup_steps: u16,
    ) -> Self {
        Self { resources_released, owned_work_remaining, cleanup_steps }
    }
    /// Returns whether all subject-owned handles/resources were released.
    #[must_use]
    pub const fn resources_released(self) -> bool {
        self.resources_released
    }
    /// Returns work remaining under the consumed subject.
    #[must_use]
    pub const fn owned_work_remaining(self) -> u16 {
        self.owned_work_remaining
    }
    /// Returns bounded cleanup operations consumed.
    #[must_use]
    pub const fn cleanup_steps(self) -> u16 {
        self.cleanup_steps
    }
}

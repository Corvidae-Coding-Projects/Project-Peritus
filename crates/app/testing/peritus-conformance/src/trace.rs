//! Runtime-neutral C7 trace and telemetry conformance contract.

mod cases;

pub use cases::trace_suite;

/// One independently exercised C7 behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceScenario {
    /// Parentage, sequence, and entity refinement remain exact.
    CausalIntegrity,
    /// Sensitive canaries are absent from every default observation surface.
    RedactionLeakage,
    /// A bounded queue accounts deterministically for capacity pressure.
    BoundedLoad,
    /// Export failure retains the exact unacknowledged batch.
    ExporterFailure,
    /// C0 replay and projection rebuild reproduce the live state.
    DurableReplay,
    /// Same-identity changed-content duplicates fail explicitly.
    DuplicateConflict,
    /// Both configured backpressure policies have exact accounting.
    Backpressure,
    /// Shutdown and restart recover the precise unacknowledged range.
    ShutdownRecovery,
    /// Observability cannot authorize or mutate execution state.
    NonAuthority,
}

/// Fixed limits supplied to one C7 conformance case.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TraceConformanceFixture {
    scenario: TraceScenario,
    queue_capacity: u32,
    maximum_batch: u32,
    maximum_shutdown_batches: u32,
    canary: &'static str,
}

impl TraceConformanceFixture {
    pub(crate) const fn new(scenario: TraceScenario) -> Self {
        Self {
            scenario,
            queue_capacity: 8,
            maximum_batch: 3,
            maximum_shutdown_batches: 2,
            canary: "peritus-c7-sensitive-canary",
        }
    }

    /// Returns the behavior under test.
    #[must_use]
    pub const fn scenario(self) -> TraceScenario {
        self.scenario
    }
    /// Returns the exact queue capacity.
    #[must_use]
    pub const fn queue_capacity(self) -> u32 {
        self.queue_capacity
    }
    /// Returns the maximum export batch size.
    #[must_use]
    pub const fn maximum_batch(self) -> u32 {
        self.maximum_batch
    }
    /// Returns the bounded shutdown flush count.
    #[must_use]
    pub const fn maximum_shutdown_batches(self) -> u32 {
        self.maximum_shutdown_batches
    }
    /// Returns the sensitive canary that must never escape default surfaces.
    #[must_use]
    pub const fn canary(self) -> &'static str {
        self.canary
    }
}

/// Direct facts observed while exercising one complete C7 scenario.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent causal, leakage, accounting, and authority facts remain explicit"
)]
pub struct TraceConformanceObservation {
    /// Observations accepted by the durable trace boundary.
    pub accepted: u64,
    /// Observations dropped or rejected by bounded backpressure.
    pub dropped: u64,
    /// Observations acknowledged by an exporter.
    pub exported: u64,
    /// Greatest in-memory queue occupancy.
    pub peak_buffered: u32,
    /// Parentage, binding refinement, and event sequencing were exact.
    pub causal_integrity: bool,
    /// Duplicate identity and digest handling was exact.
    pub duplicate_integrity: bool,
    /// Replay and rebuild produced byte-identical state.
    pub replay_equivalent: bool,
    /// Every default log/debug/error/metric/export surface excluded the canary.
    pub leakage_free: bool,
    /// Buffer accounting was monotonic and conserved every offered item.
    pub accounting_exact: bool,
    /// Failed or mismatched acknowledgements retained the pending batch.
    pub failure_retained: bool,
    /// Shutdown/restart recovered the exact unacknowledged range.
    pub recovery_exact: bool,
    /// No observation or exporter value changed authoritative execution state.
    pub non_authoritative: bool,
}

/// Stable subject failure classification.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceConformanceError {
    /// The production boundary could not be exercised or observed.
    Infrastructure,
}

/// Adapter implemented by a C7 production subject or development bridge.
pub trait TraceConformanceSubject: Send {
    /// Exercises one fixed scenario and returns direct observations.
    ///
    /// # Errors
    ///
    /// Returns [`TraceConformanceError::Infrastructure`] when setup or observation fails.
    fn exercise(
        &mut self,
        fixture: &TraceConformanceFixture,
    ) -> Result<TraceConformanceObservation, TraceConformanceError>;
}

//! Checked per-attempt cost, latency, usage, and execution observations.

/// Normalized exact resource observations; unknown values remain `None`.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResourceObservation {
    elapsed_micros: u64,
    input_tokens: Option<u64>,
    output_tokens: Option<u64>,
    cost_microunits: Option<u64>,
    memory_high_water_bytes: Option<u64>,
    cpu_micros: Option<u64>,
    process_high_water: Option<u32>,
    trace_complete: bool,
    teardown_complete: bool,
}

impl ResourceObservation {
    /// Creates one exact normalized observation.
    #[allow(
        clippy::too_many_arguments,
        reason = "independent resource observations remain explicit"
    )]
    #[must_use]
    pub const fn new(
        elapsed_micros: u64,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        cost_microunits: Option<u64>,
        memory_high_water_bytes: Option<u64>,
        cpu_micros: Option<u64>,
        process_high_water: Option<u32>,
        trace_complete: bool,
        teardown_complete: bool,
    ) -> Self {
        Self {
            elapsed_micros,
            input_tokens,
            output_tokens,
            cost_microunits,
            memory_high_water_bytes,
            cpu_micros,
            process_high_water,
            trace_complete,
            teardown_complete,
        }
    }
    /// Elapsed microseconds.
    #[must_use]
    pub const fn elapsed_micros(self) -> u64 {
        self.elapsed_micros
    }
    /// Provider input tokens when known.
    #[must_use]
    pub const fn input_tokens(self) -> Option<u64> {
        self.input_tokens
    }
    /// Provider output tokens when known.
    #[must_use]
    pub const fn output_tokens(self) -> Option<u64> {
        self.output_tokens
    }
    /// Provider cost microunits when known.
    #[must_use]
    pub const fn cost_microunits(self) -> Option<u64> {
        self.cost_microunits
    }
    /// Memory high-water bytes when observed.
    #[must_use]
    pub const fn memory_high_water_bytes(self) -> Option<u64> {
        self.memory_high_water_bytes
    }
    /// CPU microseconds when observed.
    #[must_use]
    pub const fn cpu_micros(self) -> Option<u64> {
        self.cpu_micros
    }
    /// Process-count high-water when observed.
    #[must_use]
    pub const fn process_high_water(self) -> Option<u32> {
        self.process_high_water
    }
    /// Whether required trace publication completed.
    #[must_use]
    pub const fn trace_complete(self) -> bool {
        self.trace_complete
    }
    /// Whether owned process teardown completed.
    #[must_use]
    pub const fn teardown_complete(self) -> bool {
        self.teardown_complete
    }
}

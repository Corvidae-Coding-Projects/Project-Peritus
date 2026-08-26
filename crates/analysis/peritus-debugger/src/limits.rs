//! Frozen independently tighten-able resource limits for E2 analysis.

use crate::{DebuggerError, DebuggerErrorKind, DebuggerOperation, DebuggerRecovery};

/// Every bounded E2 resource dimension.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
pub enum DebuggerLimit {
    /// Number of immutable subjects.
    Subjects = 1,
    /// Number of selected traces.
    Traces = 2,
    /// Number of selected observations.
    Events = 3,
    /// Number of retained causal edges.
    CausalEdges = 4,
    /// Number of ordinary artifact citations.
    ArtifactCitations = 5,
    /// Total verified ordinary artifact bytes.
    ArtifactBytes = 6,
    /// Number of timeline entries.
    TimelineEntries = 7,
    /// Number of report claims.
    Claims = 8,
    /// Number of causes attached to one claim.
    CausesPerClaim = 9,
    /// Number of contrary citations attached to one cause or claim.
    ContraryCitations = 10,
    /// Number of cross-run patterns.
    Patterns = 11,
    /// Number of members retained by one pattern.
    PatternMembers = 12,
    /// Number of component correlations.
    ComponentLinks = 13,
    /// Canonical report byte length.
    ReportBytes = 14,
    /// Canonical model input byte length.
    ModelInputBytes = 15,
    /// Canonical model output byte length.
    ModelOutputBytes = 16,
    /// Number of normalized model stream events.
    ModelEvents = 17,
    /// Total model tokens charged to the job.
    ModelTokens = 18,
    /// Number of model attempts.
    ModelAttempts = 19,
    /// Number of scheduled retries.
    Retries = 20,
    /// Accounted wall-time milliseconds.
    WallTimeMillis = 21,
    /// Number of retained diagnostic records.
    Diagnostics = 22,
    /// Canonical durable state byte length.
    StateBytes = 23,
    /// Canonical durable event byte length.
    EventBytes = 24,
}

impl DebuggerLimit {
    /// Complete schema-v1 dimension catalog in wire-tag order.
    pub const ALL: [Self; 24] = [
        Self::Subjects,
        Self::Traces,
        Self::Events,
        Self::CausalEdges,
        Self::ArtifactCitations,
        Self::ArtifactBytes,
        Self::TimelineEntries,
        Self::Claims,
        Self::CausesPerClaim,
        Self::ContraryCitations,
        Self::Patterns,
        Self::PatternMembers,
        Self::ComponentLinks,
        Self::ReportBytes,
        Self::ModelInputBytes,
        Self::ModelOutputBytes,
        Self::ModelEvents,
        Self::ModelTokens,
        Self::ModelAttempts,
        Self::Retries,
        Self::WallTimeMillis,
        Self::Diagnostics,
        Self::StateBytes,
        Self::EventBytes,
    ];

    /// Returns the immutable schema-v1 tag.
    #[must_use]
    pub const fn tag(self) -> u8 {
        self as u8
    }
}

/// Complete fixed resource policy for one debugger job.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DebuggerLimits {
    values: [u64; DebuggerLimit::ALL.len()],
}

impl DebuggerLimits {
    /// Compiled production ceilings. Callers can only tighten these values.
    pub const COMPILED: Self = Self {
        values: [
            256,                   // subjects
            4_096,                 // traces
            131_072,               // events
            524_288,               // causal edges
            32_768,                // artifact citations
            256 * 1024 * 1024,     // artifact bytes
            131_072,               // timeline entries
            16_384,                // claims
            64,                    // causes per claim
            256,                   // contrary citations
            8_192,                 // patterns
            4_096,                 // pattern members
            32_768,                // component links
            64 * 1024 * 1024,      // report bytes
            32 * 1024 * 1024,      // model input bytes
            16 * 1024 * 1024,      // model output bytes
            262_144,               // model events
            4_000_000,             // model tokens
            8,                     // model attempts
            7,                     // retries
            24 * 60 * 60 * 1_000,  // accounted wall time
            131_072,               // diagnostics
            16 * 1024 * 1024,      // complete C0 state-install bytes
            16 * 1024 * 1024 - 16, // payload leaves the exact B3 frame-header allowance
        ],
    };

    /// Returns compiled production ceilings.
    #[must_use]
    pub const fn production() -> Self {
        Self::COMPILED
    }

    /// Creates a policy by applying strict, duplicate-free, canonical tightenings.
    ///
    /// # Errors
    ///
    /// Rejects zero values, duplicate/descending dimensions, and values above compiled ceilings.
    pub fn tightened(overrides: &[(DebuggerLimit, u64)]) -> Result<Self, DebuggerError> {
        let mut limits = Self::COMPILED;
        let mut previous = None;
        for &(dimension, value) in overrides {
            if previous.is_some_and(|prior| prior >= dimension) {
                return Err(invalid("limit overrides must be strictly ordered and unique"));
            }
            let ceiling = Self::COMPILED.get(dimension);
            if value == 0 || value > ceiling {
                return Err(DebuggerError::numbers(
                    DebuggerErrorKind::Budget,
                    DebuggerOperation::ValidateQuery,
                    DebuggerRecovery::CorrectInput,
                    "job limit is zero or exceeds the compiled ceiling",
                    ceiling,
                    value,
                ));
            }
            limits.values[index(dimension)] = value;
            previous = Some(dimension);
        }
        Ok(limits)
    }

    /// Returns the active ceiling for one dimension.
    #[must_use]
    pub const fn get(self, dimension: DebuggerLimit) -> u64 {
        self.values[index(dimension)]
    }

    /// Returns all active ceilings in schema-tag order.
    #[must_use]
    pub const fn values(self) -> [u64; DebuggerLimit::ALL.len()] {
        self.values
    }

    /// Returns the model attempt ceiling.
    #[must_use]
    pub const fn model_attempts(self) -> u64 {
        self.get(DebuggerLimit::ModelAttempts)
    }

    /// Returns the retry ceiling.
    #[must_use]
    pub const fn retries(self) -> u64 {
        self.get(DebuggerLimit::Retries)
    }

    /// Returns the complete state byte ceiling.
    #[must_use]
    pub const fn state_bytes(self) -> u64 {
        self.get(DebuggerLimit::StateBytes)
    }

    /// Returns the complete event byte ceiling.
    #[must_use]
    pub const fn event_bytes(self) -> u64 {
        self.get(DebuggerLimit::EventBytes)
    }

    /// Checks a measured value against its active ceiling.
    ///
    /// # Errors
    ///
    /// Returns a budget error without truncating when the ceiling is exceeded.
    pub fn check(
        self,
        dimension: DebuggerLimit,
        actual: usize,
        operation: DebuggerOperation,
    ) -> Result<(), DebuggerError> {
        let actual = u64::try_from(actual).map_err(|_| {
            DebuggerError::new(
                DebuggerErrorKind::Budget,
                operation,
                DebuggerRecovery::CorrectInput,
                "resource count cannot be represented",
            )
        })?;
        let expected = self.get(dimension);
        if actual > expected {
            Err(DebuggerError::numbers(
                DebuggerErrorKind::Budget,
                operation,
                DebuggerRecovery::CorrectInput,
                "debugger resource ceiling exceeded",
                expected,
                actual,
            ))
        } else {
            Ok(())
        }
    }
}

impl Default for DebuggerLimits {
    fn default() -> Self {
        Self::production()
    }
}

const fn index(dimension: DebuggerLimit) -> usize {
    match dimension {
        DebuggerLimit::Subjects => 0,
        DebuggerLimit::Traces => 1,
        DebuggerLimit::Events => 2,
        DebuggerLimit::CausalEdges => 3,
        DebuggerLimit::ArtifactCitations => 4,
        DebuggerLimit::ArtifactBytes => 5,
        DebuggerLimit::TimelineEntries => 6,
        DebuggerLimit::Claims => 7,
        DebuggerLimit::CausesPerClaim => 8,
        DebuggerLimit::ContraryCitations => 9,
        DebuggerLimit::Patterns => 10,
        DebuggerLimit::PatternMembers => 11,
        DebuggerLimit::ComponentLinks => 12,
        DebuggerLimit::ReportBytes => 13,
        DebuggerLimit::ModelInputBytes => 14,
        DebuggerLimit::ModelOutputBytes => 15,
        DebuggerLimit::ModelEvents => 16,
        DebuggerLimit::ModelTokens => 17,
        DebuggerLimit::ModelAttempts => 18,
        DebuggerLimit::Retries => 19,
        DebuggerLimit::WallTimeMillis => 20,
        DebuggerLimit::Diagnostics => 21,
        DebuggerLimit::StateBytes => 22,
        DebuggerLimit::EventBytes => 23,
    }
}

fn invalid(detail: &'static str) -> DebuggerError {
    DebuggerError::new(
        DebuggerErrorKind::InvalidInput,
        DebuggerOperation::ValidateQuery,
        DebuggerRecovery::CorrectInput,
        detail,
    )
}

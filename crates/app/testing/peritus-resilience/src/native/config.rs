//! Validated native-controller limits and configuration failures.

use std::path::PathBuf;
use std::time::Duration;

/// Hard upper bound for one controller response document.
pub const HARD_MAX_RESPONSE_BYTES: u64 = 16 * 1024 * 1024;
/// Hard upper bound for combined controller output retained during one stage.
pub const HARD_MAX_OUTPUT_BYTES: u64 = 64 * 1024 * 1024;
/// Hard upper bound for a single native controller stage.
pub const HARD_MAX_STAGE_DURATION: Duration = Duration::from_hours(24);
/// Hard upper bound for one controller-owned process tree.
pub const HARD_MAX_PROCESSES: u32 = 4_096;

/// Invalid native H1 adapter configuration.
#[derive(Debug, thiserror::Error)]
pub enum NativeAdapterError {
    /// A required filesystem path could not be inspected or prepared.
    #[error("native H1 {operation} failed at {}: {source}", path.display())]
    Filesystem {
        /// Stable operation name.
        operation: &'static str,
        /// Affected path.
        path: PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A selected path had the wrong file type.
    #[error("native H1 {label} is not {expected}: {}", path.display())]
    PathType {
        /// Human-readable path role.
        label: &'static str,
        /// Required path type.
        expected: &'static str,
        /// Rejected path.
        path: PathBuf,
    },
    /// A process bound was zero or exceeded its hard ceiling.
    #[error("native H1 limit {field}={value} is outside 1..={maximum}")]
    Limit {
        /// Stable field name.
        field: &'static str,
        /// Rejected value.
        value: u64,
        /// Hard maximum.
        maximum: u64,
    },
}

impl NativeAdapterError {
    pub(super) fn filesystem(
        operation: &'static str,
        path: impl Into<PathBuf>,
        source: std::io::Error,
    ) -> Self {
        Self::Filesystem { operation, path: path.into(), source }
    }
}

/// Wall-clock and process bounds enforced around every controller stage.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeControllerLimits {
    stage_duration: Duration,
    response_bytes: u64,
    output_bytes: u64,
    processes: u32,
}

impl NativeControllerLimits {
    /// Creates nonzero native bounds below their fixed hard ceilings.
    ///
    /// # Errors
    ///
    /// Returns [`NativeAdapterError::Limit`] for a zero or excessive value.
    pub fn new(
        stage_duration: Duration,
        response_bytes: u64,
        output_bytes: u64,
        processes: u32,
    ) -> Result<Self, NativeAdapterError> {
        let duration_millis = u64::try_from(stage_duration.as_millis()).unwrap_or(u64::MAX);
        validate(
            "stage_duration_millis",
            duration_millis,
            u64::try_from(HARD_MAX_STAGE_DURATION.as_millis()).unwrap_or(u64::MAX),
        )?;
        validate("response_bytes", response_bytes, HARD_MAX_RESPONSE_BYTES)?;
        validate("output_bytes", output_bytes, HARD_MAX_OUTPUT_BYTES)?;
        validate("processes", u64::from(processes), u64::from(HARD_MAX_PROCESSES))?;
        Ok(Self { stage_duration, response_bytes, output_bytes, processes })
    }

    /// Returns the monotonic duration allowed for one controller stage.
    #[must_use]
    pub const fn stage_duration(self) -> Duration {
        self.stage_duration
    }

    /// Returns the maximum response document size.
    #[must_use]
    pub const fn response_bytes(self) -> u64 {
        self.response_bytes
    }

    /// Returns the combined output allowance per stage.
    #[must_use]
    pub const fn output_bytes(self) -> u64 {
        self.output_bytes
    }

    /// Returns the maximum active controller process count.
    #[must_use]
    pub const fn processes(self) -> u32 {
        self.processes
    }
}

impl Default for NativeControllerLimits {
    fn default() -> Self {
        Self {
            stage_duration: Duration::from_mins(15),
            response_bytes: 256 * 1024,
            output_bytes: 4 * 1024 * 1024,
            processes: 64,
        }
    }
}

const fn validate(field: &'static str, value: u64, maximum: u64) -> Result<(), NativeAdapterError> {
    if value == 0 || value > maximum {
        Err(NativeAdapterError::Limit { field, value, maximum })
    } else {
        Ok(())
    }
}

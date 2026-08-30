//! Hard resource bounds for one native H2 controller invocation.

use std::time::Duration;

use crate::{QualificationError, QualificationErrorCode, QualificationRecovery};

/// Resource limits applied independently to every fresh platform subject.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NativeControllerLimits {
    duration: Duration,
    output_bytes: u64,
    response_bytes: u64,
    artifact_bytes: u64,
    package_artifact_bytes: u64,
    processes: u32,
}

impl NativeControllerLimits {
    /// Creates explicit controller limits.
    ///
    /// # Errors
    ///
    /// Rejects zero values and a response bound larger than the total output bound.
    pub fn new(
        duration: Duration,
        output_bytes: u64,
        response_bytes: u64,
        artifact_bytes: u64,
        package_artifact_bytes: u64,
        processes: u32,
    ) -> Result<Self, QualificationError> {
        if duration.is_zero()
            || output_bytes == 0
            || response_bytes == 0
            || artifact_bytes == 0
            || package_artifact_bytes == 0
            || processes == 0
            || response_bytes > output_bytes
        {
            return Err(QualificationError::new(
                QualificationErrorCode::InvalidInput,
                QualificationRecovery::CorrectInput,
                "configure native H2 controller",
                "controller limits must be nonzero and response bytes must fit output bytes",
            ));
        }
        Ok(Self {
            duration,
            output_bytes,
            response_bytes,
            artifact_bytes,
            package_artifact_bytes,
            processes,
        })
    }

    pub(super) const fn duration(self) -> Duration {
        self.duration
    }

    pub(super) const fn output_bytes(self) -> u64 {
        self.output_bytes
    }

    pub(super) const fn response_bytes(self) -> u64 {
        self.response_bytes
    }

    pub(super) const fn artifact_bytes(self) -> u64 {
        self.artifact_bytes
    }

    pub(super) const fn package_artifact_bytes(self) -> u64 {
        self.package_artifact_bytes
    }

    pub(super) const fn processes(self) -> u32 {
        self.processes
    }
}

impl Default for NativeControllerLimits {
    fn default() -> Self {
        Self {
            duration: Duration::from_mins(30),
            output_bytes: 8 * 1024 * 1024,
            response_bytes: 512 * 1024,
            artifact_bytes: 64 * 1024 * 1024,
            package_artifact_bytes: 2 * 1024 * 1024 * 1024,
            processes: 128,
        }
    }
}

//! Fresh native-process subjects for production H0 adapters.

mod config;
mod process;
mod process_tree;
mod protocol;
mod subject;

pub use config::HostFingerprint;
pub use subject::NativeProbeFactory;

use crate::{QualificationError, QualificationErrorCode, QualificationRecovery};

fn native_error(operation: &'static str, detail: impl Into<String>) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::NativeExecution,
        QualificationRecovery::RepairAdapter,
        operation,
        detail,
    )
}

fn cleanup_error(operation: &'static str, detail: impl Into<String>) -> QualificationError {
    QualificationError::new(
        QualificationErrorCode::Cleanup,
        QualificationRecovery::ReplaceSubject,
        operation,
        detail,
    )
}

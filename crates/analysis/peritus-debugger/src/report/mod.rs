//! Checked report claims, complete validation, and canonical bytes.

mod canonical;
mod claim;
mod validation;

pub use claim::{ClaimKind, ReportClaim};
pub use validation::{
    DebuggerReport, ReportArtifactPage, ReportContinuationCursor, ValidatedReport, validate_report,
};

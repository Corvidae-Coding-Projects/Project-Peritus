//! E1 component correlation and diagnostic-only harness health.

mod health;
mod mapping;

pub use health::{DiagnosticStatus, HarnessHealthSummary, HealthCategoryCount, summarize_health};
#[allow(
    clippy::redundant_pub_crate,
    reason = "this internal table is shared with clustering but excluded from the public facade"
)]
pub(crate) use mapping::component_kinds_for_category;
pub use mapping::{ComponentCorrelation, ConstraintLevel, CorrelationBasis, map_components};

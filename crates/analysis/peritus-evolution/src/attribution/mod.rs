//! Deterministic prediction falsification over admitted E3 analysis.

mod engine;
mod record;

pub use engine::attribute;
pub use record::{
    AttributionEntry, AttributionRecord, AttributionUnavailable, FalsificationVerdict,
    MetricObservation,
};

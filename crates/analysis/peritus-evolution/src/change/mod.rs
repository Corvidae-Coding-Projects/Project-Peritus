//! Immutable change manifests, component deltas, predictions, and isolated variants.

mod delta;
mod manifest;
mod prediction;
mod text;
mod variant;

pub use delta::{CompatibilityEffect, ComponentDelta};
pub use manifest::ChangeManifest;
pub use prediction::{
    MetricValue, Prediction, PredictionDirection, PredictionMetric, PredictionSubject,
};
pub use text::BoundedText;
pub use variant::VariantDefinition;

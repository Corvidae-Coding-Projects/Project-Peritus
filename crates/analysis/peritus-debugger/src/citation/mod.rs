//! Manifest-contained source and ordinary-artifact citations.

mod artifact;
mod validation;

pub use artifact::{ArtifactCitation, EvidenceCitation};
pub use validation::validate_citations;

//! Deterministic analyzer registry and bounded root-cause candidates.

mod analyzer;
mod candidate;
mod confidence;
mod rules;

pub use analyzer::{AnalysisFinding, AnalyzerSignature, DeterministicAnalysis, analyze_timelines};
pub use candidate::{
    AlternativeCauses, AmbiguityFlag, CauseDerivation, DiagnosticText, RootCauseCandidate,
    UnsupportedConclusion, UnsupportedReason,
};
pub use confidence::{ConfidenceBasis, ConfidenceMillionths};

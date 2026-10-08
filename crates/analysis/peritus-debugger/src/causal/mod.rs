//! Deterministic analyzer registry and bounded root-cause candidates.

mod analyzer;
mod candidate;
mod confidence;
mod rules;

pub use analyzer::{
    AnalysisFinding, AnalyzerSignature, DeterministicAnalysis, analyze_timelines,
    analyze_timelines_controlled,
};
pub use candidate::{
    AlternativeCauses, AmbiguityFlag, CauseDerivation, DiagnosticText, DiagnosticTextCursor,
    DiagnosticTextPage, RootCauseCandidate, UnsupportedConclusion, UnsupportedReason,
};
pub use confidence::{ConfidenceBasis, ConfidenceMillionths};

//! Explicit quality-check discovery, invocation, and candidate B2 evidence.

mod acceptance;
mod catalog;
mod definition;
mod discovery;
mod dispatcher;
mod error;
mod execution;
mod input;
mod json_value;
mod observation;
mod parser;
mod plan;
mod render;
mod result;
mod snapshot;

pub use acceptance::QualityAcceptanceBinding;
pub use catalog::{discover_descriptor, run_descriptor};
pub use definition::{
    CheckDefinition, CheckRequirement, CheckSource, EnvironmentProfile, ExpectedSuccess,
    OutputParser,
};
pub use discovery::{CheckCatalog, DiscoveredCheck};
pub use dispatcher::{QualityDiscoverDispatcher, QualityRunDispatcher};
pub use error::{QualityError, QualityErrorKind};
pub use input::RunInput;
pub use observation::{CandidateGateObservation, QualityExecutionObservation};
pub use plan::QualityPlanInputs;
pub use result::{
    QualityArtifact, QualityResultBinding, QualityTerminal, QualityTerminalKind,
    decode_quality_result,
};
pub use snapshot::CleanQualitySnapshot;

//! Executable H3 load and soak qualification for integrated Peritus subjects.
//!
//! [`PacedRunner`] is deliberately tied to monotonic wall-clock time. Fast deterministic tests use
//! private injection seams and therefore cannot be substituted into a production qualification.

mod cancellation;
mod error;
mod machine;
mod runner;

pub use cancellation::CancellationFlag;
pub use error::RunnerError;
pub use machine::{MachineAssessment, MachineMismatch, MachineObservation};
pub use runner::PacedRunner;

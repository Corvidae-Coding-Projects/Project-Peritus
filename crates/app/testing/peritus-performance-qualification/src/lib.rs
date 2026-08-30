//! Executable H3 load and soak qualification for integrated Peritus subjects.
//!
//! [`PacedRunner`] is deliberately tied to monotonic wall-clock time. Fast deterministic tests use
//! private injection seams and therefore cannot be substituted into a production qualification.

#[cfg(unix)]
mod a3;
mod cancellation;
#[cfg(unix)]
mod daemon;
#[cfg(unix)]
mod effects;
mod error;
#[cfg(unix)]
mod identity;
mod machine;
#[cfg(unix)]
mod process;
mod runner;
#[cfg(unix)]
mod scheduler;
#[cfg(unix)]
mod subject;

pub use cancellation::CancellationFlag;
pub use error::{RunnerError, SubjectError};
pub use machine::{MachineAssessment, MachineMismatch, MachineObservation};
pub use runner::PacedRunner;
#[cfg(unix)]
pub use subject::{AuthorizedSubject, IntegratedSubject, SubjectAuthorization};

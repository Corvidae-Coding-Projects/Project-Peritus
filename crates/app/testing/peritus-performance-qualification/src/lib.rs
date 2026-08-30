//! Executable H3 load and soak qualification for integrated Peritus subjects.
//!
//! [`PacedRunner`] is deliberately tied to monotonic wall-clock time. Fast deterministic tests use
//! private injection seams and therefore cannot be substituted into a production qualification.

#[cfg(unix)]
mod a3;
#[cfg(unix)]
mod campaign;
#[cfg(all(test, unix))]
mod campaign_tests;
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
mod sampling;
#[cfg(unix)]
mod scheduler;
mod shared_accounting;
#[cfg(unix)]
mod subject;

#[cfg(unix)]
pub use campaign::{CampaignCoordinator, CampaignMode, CampaignOutcome, CampaignRequest};
pub use cancellation::CancellationFlag;
pub use error::{CampaignError, RunnerError, SubjectError};
pub use machine::{MachineAssessment, MachineMismatch, MachineObservation};
pub use runner::PacedRunner;
#[cfg(unix)]
pub use subject::{AuthorizedSubject, IntegratedSubject, SubjectAuthorization};

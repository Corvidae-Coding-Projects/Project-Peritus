//! Executable H3 load and soak qualification for integrated Peritus subjects.
//!
//! [`PacedRunner`] is deliberately tied to monotonic wall-clock time. Fast deterministic tests use
//! private injection seams and therefore cannot be substituted into a production qualification.

#[cfg(unix)]
mod a3;
#[cfg(unix)]
mod baseline_candidate;
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
mod evidence;
#[cfg(unix)]
mod evidence_io;
#[cfg(unix)]
mod file_digest;
#[cfg(unix)]
mod identity;
mod machine;
#[cfg(unix)]
mod operator;
#[cfg(unix)]
mod probe;
#[cfg(unix)]
mod process;
mod runner;
#[cfg(unix)]
mod sampling;
#[cfg(unix)]
mod scheduler;
#[cfg(unix)]
mod shared_accounting;
#[cfg(unix)]
mod subject;

#[cfg(unix)]
pub use campaign::{CampaignCoordinator, CampaignMode, CampaignOutcome, CampaignRequest};
pub use cancellation::CancellationFlag;
pub use error::{
    CampaignError, EvidenceError, MachineProbeError, OperatorError, RunnerError, SubjectError,
};
#[cfg(unix)]
pub use evidence::{CampaignEvidenceWriter, PublishedEvidence};
#[cfg(unix)]
pub use file_digest::sha256_file;
pub use machine::{MachineAssessment, MachineMismatch, MachineObservation, RawMachineFacts};
#[cfg(unix)]
pub use operator::{OPERATOR_USAGE, OperatorOptions};
#[cfg(unix)]
pub use probe::MachineProbe;
pub use runner::PacedRunner;
#[cfg(unix)]
pub use subject::{AuthorizedSubject, IntegratedSubject, SubjectAuthorization};

//! Production-process support for the runtime-neutral G0 conformance suite.

mod adapter;
mod artifact;
mod blob;
mod command;
mod error;
mod lease;
mod lifecycle;
mod outbox;
mod process;
mod prompt;
mod session;
mod snapshot;
mod subscription;
mod terminal;
mod wire;

pub use adapter::{BinaryDaemonFactory, BinaryDaemonSubject, blocker_for, reachable_scenarios};
pub use blob::commit_crash_recovery as blob_commit_crash_recovery;
use error::debug_error;
pub use lease::commit_crash_recovery as lease_commit_crash_recovery;
pub use outbox::journal_before_crash_recovery;
pub use snapshot::commit_crash_recovery as snapshot_commit_crash_recovery;

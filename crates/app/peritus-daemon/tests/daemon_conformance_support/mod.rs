//! Production-process support for the runtime-neutral G0 conformance suite.

mod adapter;
mod artifact;
mod blob;
mod command;
mod error;
mod lifecycle;
mod outbox;
mod process;
mod prompt;
mod session;
mod subscription;
mod terminal;
mod wire;

pub use adapter::{BinaryDaemonFactory, BinaryDaemonSubject, blocker_for, reachable_scenarios};
pub use blob::commit_crash_recovery as blob_commit_crash_recovery;
use error::debug_error;
pub use outbox::journal_before_crash_recovery;

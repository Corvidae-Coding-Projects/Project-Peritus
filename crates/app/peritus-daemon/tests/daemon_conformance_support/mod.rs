//! Production-process support for the runtime-neutral G0 conformance suite.

mod adapter;
mod artifact;
mod command;
mod error;
mod lifecycle;
mod process;
mod prompt;
mod session;
mod subscription;
mod wire;

pub(crate) use adapter::{
    BinaryDaemonFactory, BinaryDaemonSubject, blocker_for, reachable_scenarios,
};
use error::debug_error;

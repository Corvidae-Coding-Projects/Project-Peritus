//! Runtime-neutral, deterministic conformance-suite contracts and execution.
//! Cases receive fresh subjects in stable identifier order and retain typed setup, assertion,
//! panic, and teardown failures. Report text is bounded, and only a nonempty passed suite proves
//! conformance. Pending-run cancellation drops owned work in place, so subjects must be RAII-safe.

mod agent;
mod catalog;
mod collaboration;
mod contracts;
mod descriptor;
mod facade;
mod failure;
mod gate;
mod identity;
mod journal;
mod outcome;
mod process;
mod provider;
mod replay;
mod report;
mod review;
mod runner;
mod sandbox;
mod scheduler;
mod text;
mod tool;
mod trace;
mod unwind;
mod workspace;

pub use facade::*;

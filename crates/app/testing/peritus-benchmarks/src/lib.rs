//! Deterministic performance, load, and soak qualification for Peritus.
//!
//! This crate owns measurement and qualification mechanics, not product authority. A G0 daemon or
//! F0 evolution adapter supplies its own authorization type to the subject contract. The harness
//! cannot construct, widen, or approve that authorization and its verdict is evidence for a
//! release gate rather than a release transition.

mod accounting;
#[cfg(test)]
mod accounting_tests;
mod accounting_types;
mod baseline;
mod baseline_dataset;
mod contracts;
mod dataset;
mod error;
mod evaluation;
mod evaluation_types;
mod evidence;
mod identity;
mod measurement;
mod metric;
mod plan;
mod plan_iterator;
#[cfg(test)]
mod plan_tests;
mod profile;
mod profile_resources;
mod report;
mod workload;

pub use accounting::*;
pub use accounting_types::*;
pub use baseline::*;
pub use baseline_dataset::*;
pub use contracts::*;
pub use dataset::*;
pub use error::*;
pub use evaluation::*;
pub use evaluation_types::*;
pub use evidence::*;
pub use identity::*;
pub use measurement::*;
pub use metric::*;
pub use plan::*;
pub use plan_iterator::*;
pub use profile::*;
pub use profile_resources::*;
pub use report::*;
pub use workload::*;

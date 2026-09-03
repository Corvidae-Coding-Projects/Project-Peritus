//! Verified candidate-checkpoint and terminal-settlement domain for Peritus.
//!
//! The crate accepts already-observed facts and derives what a run may truthfully claim. It owns no
//! filesystem, provider, process, clock, persistence, protocol, or user-interface effects.

use vstd::prelude::*;

verus! {

mod cause;
mod checkpoint;
mod disposition;
mod error;
mod evidence;
mod identity;
mod reducer;
mod settlement;
mod stage;
pub mod verified;

pub use cause::SettlementCause;
pub use checkpoint::CandidateCheckpoint;
pub use disposition::RunDisposition;
pub use error::{SettlementError, SettlementErrorKind};
pub use evidence::{EvidenceRecord, EvidenceStatus, QualificationEvidence};
pub use identity::CandidateIdentity;
pub use reducer::SettlementReducer;
pub use settlement::RunSettlement;
pub use stage::CandidateStage;

} // verus!

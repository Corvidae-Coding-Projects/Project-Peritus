//! Verified immutable acceptance contracts for Peritus.
//!
//! This crate validates contract identity, gate dependency graphs, review policy, evidence
//! declarations, and exact [`peritus_types::RevisionTuple`] binding. It performs no I/O and does
//! not interpret evidence.

use vstd::prelude::*;

verus! {

mod contract;
mod documents;
mod error;
mod evidence;
mod gate;
mod gate_model;
mod identity;
mod policy;
mod proofs;
mod requirement;
mod review;

pub use contract::{AcceptanceContract, ContractBinding};
pub use documents::ContractDocuments;
pub use error::{CanonicalCollection, LimitKind, SpecError, SpecErrorKind};
pub use evidence::{
    EvidenceRequirement, EvidenceSource, ExportClassification, HumanApprovalPolicy, WaiverPolicy,
};
pub use gate::{
    GateDefinition, GateExecutionPlan, GateFreshnessScope, GateGraph, GateSuccessRule,
};
#[cfg(verus_only)]
pub use gate_model::gate_execution_order_is_valid;
pub use identity::{ContentReference, EvidenceRequirementId, RequirementId, ReviewCategory};
#[cfg(verus_only)]
pub use identity::acceptance_ids_match;
pub use policy::CompletionPolicy;
pub use requirement::{Assumption, Exclusion, Requirement};
pub use review::{FindingSeverity, ReviewPolicy, ReviewerIndependence};

} // verus!

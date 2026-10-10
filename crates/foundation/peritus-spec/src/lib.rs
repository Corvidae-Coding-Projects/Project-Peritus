//! Verified immutable acceptance contracts for Peritus.
//!
//! This crate validates contract identity, gate dependency graphs, review policy, evidence
//! declarations, and exact [`peritus_types::RevisionTuple`] binding. It performs no I/O and does
//! not interpret evidence.

mod contract;
mod documents;
mod error;
mod evidence;
mod gate;
mod gate_model;
mod identity;
mod policy;
mod requirement;
mod review;
pub use contract::{AcceptanceContract, ContractBinding};
pub use documents::ContractDocuments;
pub use error::{CanonicalCollection, LimitKind, SpecError, SpecErrorKind};
pub use evidence::{
    EvidenceRequirement, EvidenceSource, ExportClassification, HumanApprovalPolicy, WaiverPolicy,
};
pub use gate::{GateDefinition, GateExecutionPlan, GateFreshnessScope, GateGraph, GateSuccessRule};
pub use identity::{ContentReference, EvidenceRequirementId, RequirementId, ReviewCategory};
pub use policy::CompletionPolicy;
pub use requirement::{Assumption, Exclusion, Requirement};
pub use review::{FindingSeverity, ReviewPolicy, ReviewerIndependence};

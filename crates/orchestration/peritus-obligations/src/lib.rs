//! Verified public requirement obligations, qualification evidence, and failure ownership.

mod browser;
mod canonical;
mod condition;
mod error;
mod evidence;
mod failure;
mod identity;
mod ledger;
mod lifecycle;
mod limits;
mod path;
mod performance;
mod provenance;
mod qualification;
mod requirement;
mod schema;
mod verified;

pub use browser::{BrowserEvidence, BrowserImplementation, BrowserRequirement};
pub use condition::{ConditionObservation, ConditionState};
pub use error::{ObligationError, ObligationErrorKind};
pub use evidence::ExternalEffectEvidence;
pub use evidence::{DirectEvidence, EvidenceBinding, RequirementEvidence};
pub use failure::{FailureContext, FailureDisposition, FailureOwner};
pub use identity::{AlternativeBranchId, AlternativeGroupId, ConditionId, PathId, SchemaFieldId};
pub use ledger::{RequirementDraft, RequirementLedger};
pub use lifecycle::{LifecycleEvidence, LifecycleObservationKind, LifecycleRequirement};
pub use limits::ObligationLimits;
pub use path::{PathMention, PathRole};
pub use performance::{
    PerformanceEvidence, PerformanceExpectation, PerformanceRequirement, PerformanceStatistic,
};
pub use provenance::{ClauseProvenance, PublicClause, PublicTaskSource};
pub use qualification::{EvidenceVerdict, QualificationReport, qualify};
pub use requirement::{ObligationSpec, RequirementClass, RequirementEntry};
pub use schema::{SchemaDirection, SchemaEvidence, SchemaField, SchemaRequirement};
pub use verified::{
    alternative_group_complete, conditional_obligation_active, failure_authorizes_fixer,
    qualification_allowed,
};

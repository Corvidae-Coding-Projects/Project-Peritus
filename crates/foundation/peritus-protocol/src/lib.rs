//! Stable canonical domain protocol for Peritus.
//!
//! Wire decoding establishes syntax and domain validity only. It never grants capability,
//! budget, acceptance, or durable-event authority.

mod acceptance;
mod budget;
mod lifecycle;
mod policy;
mod primitive;
pub mod schema;
mod version;

pub use budget::{
    BudgetAmountsDto, BudgetCommandDto, BudgetErrorDto, BudgetReceiptDto, BudgetSnapshotDto,
    ReservationSnapshotDto,
};
pub use lifecycle::{
    CommandEnvelopeDto, KernelCommandDto, KernelErrorDto, KernelEventDto, KernelSubjectDto,
    LifecyclePhaseDto,
};
pub use policy::{
    ActionIntentDto, ApprovalRequirementDto, AuthorityBoundaryDto, AuthorityCeilingDto,
    CeilingGrantDto, OperationDescriptorDto, PermissionDto, PolicyAmendmentConversionError,
    PolicyAmendmentDto, PolicyDefinitionDto, RestrictionLayerDto, RestrictionRuleDto,
    RestrictionRuleKindDto, ScopeSelectorDto,
};

pub use acceptance::{
    AcceptanceContractConversionError, AcceptanceContractDto, GateDefinitionDto, ReviewPolicyDto,
};
pub use version::SCHEMA_V1;

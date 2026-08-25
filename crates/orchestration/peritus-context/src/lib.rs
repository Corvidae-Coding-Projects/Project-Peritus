//! Verified provenance-aware context planning for Peritus.
//!
//! The crate is a pure control-plane library: callers supply bounded content, identities,
//! logical recency, role profiles, and token estimates. It performs no model or provider I/O.

use vstd::prelude::*;

verus! {

mod authority;
mod budget;
mod compaction;
mod content;
mod error;
mod graph;
mod identity;
mod node;
mod plan;
mod precedence;
mod provenance;
mod render;
mod selection;
mod trust;
mod verified;

pub use authority::AuthorityClass;
pub use budget::{TokenAccounting, TokenBudget};
pub use compaction::{
    CompactionPolicy, CompactionProposal, SourceRange, ValidatedCompaction, validate_compaction,
};
pub use content::{ContentKind, ContextContent, ContextLimits};
#[cfg(not(verus_only))]
pub use content::bind_context_content;
pub use error::{ContextError, ContextErrorKind};
pub use graph::ContextGraph;
pub use identity::{CompactionPolicyId, ContextNodeId, ContextPlanId};
pub use node::{ContextNode, ContextNodeMetadata, RequirementMode, RoleVisibility};
pub use plan::{
    ContextPlan, OmissionReason, OmittedContext, SelectionReason, SelectedContext,
};
pub use provenance::Provenance;
pub use render::{MessageRole, RenderPlan, RenderSegment, build_render_plan};
pub use selection::{SelectionPolicy, select_context};
pub use trust::TrustClass;
pub use verified::{plan_dependencies_complete, plan_is_visible, token_accounting_is_bounded};

} // verus!

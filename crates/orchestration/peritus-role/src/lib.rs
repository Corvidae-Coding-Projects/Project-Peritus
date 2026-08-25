//! Verified role-aware context policy for Peritus.
//!
//! This crate projects B1 roles into narrower context and capability views. It cannot issue or use
//! capabilities and does not redefine the canonical security role.

use vstd::prelude::*;

verus! {

mod capability_view;
mod context_class;
mod context_policy;
mod error;
mod harness_role;
mod independence;
mod presentation;
mod verified;

pub use capability_view::CapabilityView;
pub use context_class::{ContextClass, ContextClassSet};
pub use context_policy::{ContextPolicy, MemoryVisibility, ReasoningVisibility, RoleProfile};
pub use error::{RoleError, RoleErrorKind};
pub use harness_role::HarnessRole;
pub use independence::ReviewIndependenceView;
pub use presentation::{PresentationProfile, PresentationStyle};
pub use verified::{capability_view_is_narrow, reviewer_context_is_fresh};

} // verus!

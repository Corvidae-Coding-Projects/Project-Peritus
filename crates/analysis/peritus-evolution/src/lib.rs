//! Evidence-backed production harness evolution for Peritus.
//!
//! F0 validates immutable evidence, attributes declared changes, selects candidates under a frozen
//! deny-wins policy, and owns durable campaign and production-pointer transitions. Deterministic
//! decisions remain pure; narrow adapters commit their observations through existing artifact,
//! evidence, approval, kernel, and journal owners.

#![allow(
    clippy::large_enum_variant,
    reason = "bounded closed command and state enums retain exact immutable domain values"
)]
#![allow(
    clippy::large_types_passed_by_value,
    reason = "immutable domain constructors intentionally transfer ownership into retained state"
)]
#![allow(
    clippy::redundant_pub_crate,
    reason = "crate-private helpers cross sibling boundaries through intentionally private modules"
)]
#![allow(
    clippy::too_many_arguments,
    reason = "canonical constructors keep independent digest and identity fields explicit"
)]
#![allow(
    clippy::suspicious_operation_groupings,
    reason = "binding predicates intentionally compare like-typed fields owned by different domain records"
)]

mod attribution;
mod binding;
mod campaign;
mod change;
mod durability;
mod error;
mod identity;
mod limits;
mod pointer;
#[cfg(feature = "qualification")]
pub mod qualification;
mod runtime;
mod selection;
pub mod verified;
pub mod wire;

pub use attribution::*;
pub use binding::*;
pub use campaign::*;
pub use change::*;
pub use durability::*;
pub use error::*;
pub use identity::*;
pub use limits::*;
pub use peritus_types::EvolutionCampaignId;
pub use pointer::*;
pub use runtime::*;
pub use selection::*;
pub use wire::*;

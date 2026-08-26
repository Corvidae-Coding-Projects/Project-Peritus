//! Durable evidence-linked trace diagnosis for Peritus.
//!
//! E2 selects immutable redacted C7 evidence, derives bounded deterministic and optionally
//! model-assisted diagnostic reports, and publishes them through existing C0 owners. Reports are
//! inert evidence and carry no mutation, acceptance, evaluation, waiver, or promotion authority.

mod aggregate;
mod binding;
mod causal;
mod citation;
mod clustering;
mod component;
mod durability;
mod error;
mod identity;
mod limits;
mod model;
mod projection;
mod query;
mod report;
mod runtime;
mod selection;
mod taxonomy;
mod timeline;
pub mod verified;
mod wire;

pub use aggregate::*;
pub use binding::*;
pub use causal::*;
pub use citation::*;
pub use clustering::*;
pub use component::*;
pub use durability::*;
pub use error::*;
pub use identity::*;
pub use limits::*;
pub use model::*;
pub use projection::*;
pub use query::*;
pub use report::*;
pub use runtime::*;
pub use selection::*;
pub use taxonomy::*;
pub use timeline::*;
pub use wire::*;

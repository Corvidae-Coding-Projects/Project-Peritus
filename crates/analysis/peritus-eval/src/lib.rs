//! Durable reproducible harness evaluation for Peritus.
//!
//! E3 freezes immutable inputs, plans paired bounded rollouts, retains complete outcome evidence,
//! and derives portable statistics. Its reports are inert and carry no promotion authority.

mod accounting;
mod aggregate;
mod dataset;
mod durability;
mod error;
mod execution;
pub(crate) mod identity;
mod limits;
mod plan;
mod profile;
mod projection;
mod report;
mod runtime;
mod statistics;
pub mod verified;
pub mod wire;

pub use accounting::*;
pub(crate) use aggregate::encode_kind as encode_evaluation_kind;
pub(crate) use aggregate::encode_work as encode_evaluation_work;
pub use aggregate::*;
pub use dataset::*;
pub use durability::*;
pub use error::*;
pub use execution::*;
pub use identity::*;
pub use limits::*;
pub use peritus_types::EvaluationCampaignId;
pub use plan::*;
pub use profile::*;
pub use projection::EvaluationProjection;
pub use report::*;
pub use runtime::*;
pub use statistics::*;
pub use wire::{EvaluationCommandFrame, EvaluationEventFrame, EvaluationStateFrame};

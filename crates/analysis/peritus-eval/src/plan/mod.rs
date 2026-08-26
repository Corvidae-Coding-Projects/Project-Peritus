//! Deterministic paired rollout planning and D3 work binding.

mod builder;
mod rollout;
mod seed;

pub use builder::EvaluationPlan;
pub use rollout::{RolloutSpec, SchedulingKey};
pub use seed::RolloutSeed;

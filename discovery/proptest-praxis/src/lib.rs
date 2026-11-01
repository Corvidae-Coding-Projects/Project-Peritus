//! Discovery-only consumers of Peritus's actual public library interfaces.

use std::collections::BTreeMap;

/// Behavioral event counts; each pilot defines the names and counting semantics.
pub type Metrics = BTreeMap<String, u64>;

pub mod agent_driver;
pub mod context;
pub mod framing;
pub mod invalidation;
pub mod knowledge;
pub mod memory;
mod model;
pub mod review;
pub mod settlement;
pub mod stage4;
pub mod stage5;
pub mod stage6;
pub mod stage7;
pub mod telemetry;

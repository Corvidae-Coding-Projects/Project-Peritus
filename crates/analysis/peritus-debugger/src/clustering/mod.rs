//! Deterministic exact-fingerprint and bounded agglomerative clustering.

mod engine;
mod fingerprint;

pub use engine::{PatternCluster, PatternKind, PatternMember, cluster_findings};
pub use fingerprint::PatternFingerprint;

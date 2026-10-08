//! Deterministic exact-fingerprint and bounded agglomerative clustering.

mod engine;
mod fingerprint;

pub use engine::{
    PatternCluster, PatternKind, PatternMember, cluster_findings, cluster_findings_controlled,
};
pub use fingerprint::PatternFingerprint;

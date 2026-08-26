//! Portable bounded E3 statistical calculations.

mod distribution;
mod interval;
mod paired;
mod pass_at_k;
mod stability;

pub use distribution::DistributionSummary;
pub use interval::WilsonInterval;
pub use paired::{
    BootstrapInterval, PairedCell, PairedComparison, PairedTable, SignTest, compare_paired,
};
pub use pass_at_k::{PassAtK, ProbabilityMillionths, pass_at_k};
pub use stability::{StabilityClass, StabilitySummary, analyze_stability};

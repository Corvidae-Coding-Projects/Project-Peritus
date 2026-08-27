//! Typed runner failures that remain distinct from subject verdicts.

use peritus_benchmarks::QualificationError;

/// Failure to execute or account one qualification plan.
#[derive(Debug, thiserror::Error)]
pub enum RunnerError<E>
where
    E: std::error::Error + Send + Sync + 'static,
{
    /// The integrated subject rejected or failed one planned operation.
    #[error("qualification subject failed at plan step {step}")]
    Subject {
        /// Zero-based plan step that failed.
        step: u64,
        /// Typed component-owned failure.
        #[source]
        source: E,
    },
    /// The core receipt contract rejected the terminal runner state.
    #[error("qualification receipt was invalid")]
    Receipt(#[source] QualificationError),
}

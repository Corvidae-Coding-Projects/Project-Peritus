//! Native H1 release-candidate operator and platform effect boundary.

mod args;
mod digest;
mod operator;
mod publication;

pub use operator::{H1OperatorStatus, run_h1_operator};

//! Independent exact-state and exact-output oracle for generated lease traces.

mod record;
mod state;
mod use_output;

pub use record::{ExpectedBinding, ExpectedTransition, assert_transition};
pub use state::ReferenceState;
pub use use_output::ExpectedUseOutput;

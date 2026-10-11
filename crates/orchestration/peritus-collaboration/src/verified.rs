//! Executable collaboration invariants and focused Verus proof roots.

mod executable;

pub use executable::{
    cancellation_dominates, causal_graph_is_valid, join_is_truthful, replay_equivalent,
    terminal_is_truthful, transition_is_legal,
};

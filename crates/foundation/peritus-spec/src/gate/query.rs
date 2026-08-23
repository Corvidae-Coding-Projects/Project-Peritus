//! Read-only graph queries over the validated stored execution order.

use super::GateGraph;
use peritus_types::GateId;
use vstd::prelude::*;

verus! {

fn gate_position(values: &[GateId], target: GateId) -> Option<usize> {
    let mut index = 0;
    while index < values.len()
        invariant index <= values.len(),
        decreases values.len() - index,
    {
        if values[index] == target { return Some(index); }
        index += 1;
    }
    None
}

impl GateGraph {
    /// Returns whether `dependency` is scheduled strictly before `gate`.
    ///
    /// A checked graph returns `true` for every declared dependency edge. Unknown identifiers
    /// return `false`.
    #[must_use]
    pub fn dependency_precedes(&self, dependency: GateId, gate: GateId) -> bool {
        match (
            gate_position(self.execution_order.as_slice(), dependency),
            gate_position(self.execution_order.as_slice(), gate),
        ) {
            (Some(dependency_position), Some(gate_position)) => {
                dependency_position < gate_position
            }
            _ => false,
        }
    }
}

} // verus!

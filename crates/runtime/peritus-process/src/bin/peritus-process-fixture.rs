//! Process-behavior fixture used by the crate's production adapter tests.

#[allow(unused_imports, reason = "Verus verifies every crate target through this prelude")]
use vstd::prelude::*;

#[path = "peritus-process-fixture/behavior.rs"]
mod behavior;

fn main() {
    behavior::run();
}

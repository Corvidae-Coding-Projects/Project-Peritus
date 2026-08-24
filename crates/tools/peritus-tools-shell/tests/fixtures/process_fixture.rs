//! Composition root for observable C4 process test behavior.

#[allow(unused_imports, reason = "Verus verifies every crate target through this prelude")]
use vstd::prelude::*;

mod target_behavior;

fn main() {
    target_behavior::run();
}

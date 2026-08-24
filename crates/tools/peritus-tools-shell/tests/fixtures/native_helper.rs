//! Composition root for the C4 native protocol test helper.

#[allow(unused_imports, reason = "Verus verifies every crate target through this prelude")]
use vstd::prelude::*;

mod native_protocol;

fn main() {
    if native_protocol::exchange_and_exec().is_err() {
        std::process::exit(125);
    }
}

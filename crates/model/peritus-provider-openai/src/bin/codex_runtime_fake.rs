//! Portable hermetic executable used only by this package's runtime conformance tests.

#[allow(unused_imports, reason = "Verus verifies every crate target through this prelude")]
use vstd::prelude::*;

#[path = "codex_runtime_fake/mod.rs"]
mod app;

fn main() {
    app::run();
}

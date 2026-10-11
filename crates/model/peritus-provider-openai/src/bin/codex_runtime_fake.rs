//! Portable hermetic executable used only by this package's runtime conformance tests.

#[path = "codex_runtime_fake/mod.rs"]
mod app;

fn main() {
    app::run();
}

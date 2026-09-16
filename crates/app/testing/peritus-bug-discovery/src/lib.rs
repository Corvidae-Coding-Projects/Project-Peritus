//! Bounded test-only discovery adapters shared by libFuzzer and stable corpus replay.

mod framing;
mod provider;
mod target;
mod working;
pub use target::{DiscoveryTarget, MAX_INPUT_BYTES, check_input};

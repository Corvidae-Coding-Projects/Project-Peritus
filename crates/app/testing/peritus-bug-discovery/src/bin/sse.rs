//! Bounded raw SSE target.
#![cfg_attr(all(not(test), not(windows)), no_main)]

#[cfg(all(not(test), not(windows)))]
#[path = "sse/fuzz_entry.rs"]
mod fuzz_entry;

#[cfg(all(not(test), windows))]
fn main() {
    eprintln!("the SSE libFuzzer target is unsupported on Windows");
    std::process::exit(2);
}

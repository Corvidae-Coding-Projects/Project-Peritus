//! Bounded structured working-state target.
#![cfg_attr(all(not(test), not(windows)), no_main)]

#[cfg(all(not(test), not(windows)))]
#[path = "working_state/fuzz_entry.rs"]
mod fuzz_entry;

#[cfg(all(not(test), windows))]
fn main() {
    eprintln!("the working-state libFuzzer target is unsupported on Windows");
    std::process::exit(2);
}

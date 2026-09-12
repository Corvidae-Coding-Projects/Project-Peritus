//! Bounded structured normalized-provider sequence target.
#![cfg_attr(all(not(test), not(windows)), no_main)]

#[cfg(all(not(test), not(windows)))]
#[path = "provider_sequence/fuzz_entry.rs"]
mod fuzz_entry;

#[cfg(all(not(test), windows))]
fn main() {
    eprintln!("the provider-sequence libFuzzer target is unsupported on Windows");
    std::process::exit(2);
}

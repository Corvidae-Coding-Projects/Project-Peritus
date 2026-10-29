//! Operate the actual host promise check on a supplied public reply; no model or tool substitutes.

#[path = "../../../../crates/app/peritus-product-runner/src/execution/conversation/handoff.rs"]
mod handoff;

use std::io::{self, Read as _};

fn main() -> io::Result<()> {
    let mut reply = String::new();
    io::stdin().read_to_string(&mut reply)?;
    // Keep the check and its corrective policy bound to the same production source.
    let _ = (handoff::POLICY, handoff::CORRECTION);
    println!("{}", handoff::promises_execution(&reply));
    Ok(())
}

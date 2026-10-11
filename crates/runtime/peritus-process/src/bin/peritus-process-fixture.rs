//! Process-behavior fixture used by the crate's production adapter tests.

#[path = "peritus-process-fixture/behavior.rs"]
mod behavior;

fn main() {
    behavior::run();
}

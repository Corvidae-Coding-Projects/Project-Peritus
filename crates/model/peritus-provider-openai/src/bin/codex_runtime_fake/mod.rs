//! Composition of the test-only executable concerns.

mod behavior;
mod contract;
mod scenario;
mod trace;

pub fn run() {
    behavior::run();
}

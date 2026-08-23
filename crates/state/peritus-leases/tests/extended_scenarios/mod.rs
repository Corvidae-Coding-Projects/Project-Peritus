//! Generated exact transition scenarios.

mod accepted;
mod rejected;

pub fn run(seed: u8) {
    accepted::run_foundation(seed);
    for case in 0..9_u8 {
        accepted::run(seed, case);
        rejected::run(seed, case);
    }
}

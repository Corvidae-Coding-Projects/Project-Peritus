//! Generated exact traces against an independent full-state and full-output oracle.

mod extended_reference;
mod extended_scenarios;
mod support;

#[test]
fn generated_extended_transitions_refine_independent_model() {
    for seed in 1..=16_u8 {
        extended_scenarios::run(seed);
    }
}

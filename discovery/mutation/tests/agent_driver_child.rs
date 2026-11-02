use std::path::Path;

use peritus_praxis::agent_driver::{Case, Cut, Ending, check_with_child};

#[test]
fn mutation_bridge_reaches_provider_intent_crash_cut() {
    let case = Case {
        tools: 3,
        retries: 1,
        completed_before_cut: 1,
        cut: Cut::ProviderIntent,
        ending: Ending::Complete,
        pause: true,
        reopens: 2,
    };

    check_with_child(&case, Path::new(env!("CARGO_BIN_EXE_peritus-mutation-agent-driver-child")))
        .unwrap();
}

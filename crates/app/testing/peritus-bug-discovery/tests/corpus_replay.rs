//! Checked-in corpora replay with the production stable toolchain.

use peritus_bug_discovery::{DiscoveryTarget, check_input};

#[test]
fn seed_corpora_replay() {
    for (target, corpus) in [
        (DiscoveryTarget::Sse, include_bytes!("../corpus/sse/basic").as_slice()),
        (DiscoveryTarget::Sse, include_bytes!("../corpus/sse/exact-limit-crlf").as_slice()),
        (DiscoveryTarget::Ndjson, include_bytes!("../corpus/ndjson/basic").as_slice()),
        (DiscoveryTarget::Ndjson, include_bytes!("../corpus/ndjson/exact-limit-crlf").as_slice()),
        (
            DiscoveryTarget::WorkingState,
            include_bytes!("../corpus/working_state/required-chain").as_slice(),
        ),
        (
            DiscoveryTarget::ProviderSequence,
            include_bytes!("../corpus/provider_sequence/basic").as_slice(),
        ),
    ] {
        check_input(target, corpus);
    }
}

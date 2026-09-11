//! libFuzzer callback for raw SSE inputs.

libfuzzer_sys::fuzz_target!(|bytes: &[u8]| {
    if bytes.len() <= peritus_bug_discovery::MAX_INPUT_BYTES {
        peritus_bug_discovery::check_input(peritus_bug_discovery::DiscoveryTarget::Sse, bytes);
    }
});

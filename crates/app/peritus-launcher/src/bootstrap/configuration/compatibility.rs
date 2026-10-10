//! Narrow byte compatibility for immutable configuration generations from prior releases.

pub(super) fn matches_current_or_legacy_timeout_configuration(
    actual: &[u8],
    expected: &str,
) -> bool {
    if actual == expected.as_bytes() {
        return true;
    }
    // Configuration generations are immutable. Upgrades must keep accepting the exact bytes
    // generated before the obsolete provider deadline was removed instead of stranding an
    // otherwise unchanged product-state generation. No other byte difference is admitted.
    let legacy = expected.replacen(
        "\n\n[telemetry]\n",
        "\nprovider_turn_timeout_seconds = 600\n\n[telemetry]\n",
        1,
    );
    actual == legacy.as_bytes()
}

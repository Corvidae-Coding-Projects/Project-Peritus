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

#[cfg(test)]
mod tests {
    use super::matches_current_or_legacy_timeout_configuration;

    const CURRENT: &str =
        "[product]\nautomatic_provider_failover = false\n\n[telemetry]\nmode = \"disabled\"\n";

    #[test]
    fn exact_prior_generated_timeout_is_accepted_without_weakening_other_immutability() {
        let legacy = "[product]\nautomatic_provider_failover = false\nprovider_turn_timeout_seconds = 600\n\n[telemetry]\nmode = \"disabled\"\n";
        assert!(matches_current_or_legacy_timeout_configuration(CURRENT.as_bytes(), CURRENT));
        assert!(matches_current_or_legacy_timeout_configuration(legacy.as_bytes(), CURRENT));
        assert!(!matches_current_or_legacy_timeout_configuration(
            legacy.replace("600", "45").as_bytes(),
            CURRENT,
        ));
        assert!(!matches_current_or_legacy_timeout_configuration(
            CURRENT.replace("disabled", "enabled").as_bytes(),
            CURRENT,
        ));
    }
}

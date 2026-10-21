//! Narrow byte compatibility for immutable configuration generations from prior releases.

pub(super) fn matches_current_or_legacy_configuration(actual: &[u8], expected: &str) -> bool {
    if actual == expected.as_bytes() {
        return true;
    }
    // Recognize only exact earlier generated formats, including upgrades that skipped
    // releases. Never normalize arbitrary TOML, authority, paths, or provider choices.
    for discovery in [true, false] {
        let expected = if discovery {
            expected.to_owned()
        } else {
            expected.replace("\ndiscover_model_limits = true\n", "\n")
        };
        if matches_timeout_variants(actual, &expected)
            || matches_timeout_variants(actual, &legacy_opencode_capabilities(&expected))
        {
            return true;
        }
    }
    false
}

fn matches_timeout_variants(actual: &[u8], expected: &str) -> bool {
    if actual == expected.as_bytes() {
        return true;
    }
    let legacy = expected.replacen(
        "\n\n[telemetry]\n",
        "\nprovider_turn_timeout_seconds = 600\n\n[telemetry]\n",
        1,
    );
    actual == legacy.as_bytes()
}

fn legacy_opencode_capabilities(expected: &str) -> String {
    expected
        .split("\n[[providers]]\n")
        .map(|section| {
            if section.contains("hosted_service = \"opencode-zen\"")
                || section.contains("hosted_service = \"opencode-go\"")
            {
                section.replacen(
                    "capabilities = [\"parallel-tool-calls\", \"streaming\", \"tool-calls\", \"usage-detail\"]",
                    "capabilities = [\"streaming\", \"tool-calls\", \"usage-detail\"]",
                    1,
                )
            } else {
                section.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n[[providers]]\n")
}

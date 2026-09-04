//! Shared validation for exceptional reviewed Rust matrix entries.

use yaml_rust2::Yaml;

const PLATFORM_TERMINAL_ENTRIES: [(&str, &str); 9] = [
    ("ubuntu-24.04", "test-platform-terminal-interactive"),
    ("ubuntu-24.04", "test-platform-terminal-signal"),
    ("ubuntu-24.04", "test-platform-terminal-cancel"),
    ("macos-15", "test-platform-terminal-interactive"),
    ("macos-15", "test-platform-terminal-signal"),
    ("macos-15", "test-platform-terminal-cancel"),
    ("windows-2025", "test-platform-terminal-interactive"),
    ("windows-2025", "test-platform-terminal-signal"),
    ("windows-2025", "test-platform-terminal-cancel"),
];

pub(super) fn has_platform_terminal_includes(value: Option<&Yaml>) -> bool {
    let Some(entries) = value.and_then(Yaml::as_vec) else { return false };
    entries.len() == PLATFORM_TERMINAL_ENTRIES.len()
        && entries.iter().zip(PLATFORM_TERMINAL_ENTRIES).all(|(entry, expected)| {
            let Some(entry) = entry.as_hash() else { return false };
            entry.len() == 3
                && string(entry, "os") == Some(expected.0)
                && string(entry, "operation") == Some(expected.1)
                && string(entry, "shard") == Some("testing-platform")
        })
}

fn string<'a>(mapping: &'a yaml_rust2::yaml::Hash, key: &str) -> Option<&'a str> {
    mapping.get(&Yaml::String(key.to_owned())).and_then(Yaml::as_str)
}

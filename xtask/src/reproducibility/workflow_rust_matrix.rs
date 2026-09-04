//! Shared validation for exceptional reviewed Rust matrix entries.

use yaml_rust2::Yaml;

const HOSTS: [&str; 3] = ["ubuntu-24.04", "macos-15", "windows-2025"];

pub(super) fn has_platform_terminal_includes(value: Option<&Yaml>) -> bool {
    let Some(entries) = value.and_then(Yaml::as_vec) else { return false };
    entries.len() == HOSTS.len()
        && entries.iter().zip(HOSTS).all(|(entry, host)| {
            let Some(entry) = entry.as_hash() else { return false };
            entry.len() == 3
                && string(entry, "os") == Some(host)
                && string(entry, "operation") == Some("test-platform-terminal")
                && string(entry, "shard") == Some("testing-platform")
        })
}

fn string<'a>(mapping: &'a yaml_rust2::yaml::Hash, key: &str) -> Option<&'a str> {
    mapping.get(&Yaml::String(key.to_owned())).and_then(Yaml::as_str)
}

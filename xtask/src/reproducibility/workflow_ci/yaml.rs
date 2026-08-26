//! Exact YAML accessors for the canonical CI schema.

use yaml_rust2::Yaml;
use yaml_rust2::yaml::Hash;

pub(super) fn exact_keys(mapping: &Hash, expected: &[&str]) -> bool {
    mapping.len() == expected.len()
        && mapping.keys().all(|key| key.as_str().is_some_and(|key| expected.contains(&key)))
}

pub(super) fn string<'a>(mapping: &'a Hash, key: &str) -> Option<&'a str> {
    mapping_value(mapping, key).and_then(Yaml::as_str)
}

pub(super) fn integer(mapping: &Hash, key: &str) -> Option<i64> {
    mapping_value(mapping, key).and_then(Yaml::as_i64)
}

pub(super) fn mapping_value<'a>(mapping: &'a Hash, key: &str) -> Option<&'a Yaml> {
    mapping.get(&Yaml::String(key.to_owned()))
}

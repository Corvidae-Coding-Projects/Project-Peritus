use crate::error::Diagnostic;
use crate::model::ToolchainPolicy;
use std::path::Path;
use yaml_rust2::Yaml;
use yaml_rust2::yaml::Hash;

const RUST_PIN: &str = "RUST_VERSION";
const VERUS_PIN: &str = "VERUS_VERSION";
const ARCHIVE_DIGEST_PIN: &str = "VERUS_LINUX_SHA256";
const RUST_TOOLCHAIN_INPUTS: [&str; 2] = ["toolchain", "rust-version"];
const RUST_ENV_REFERENCE: &str = "${{ env.RUST_VERSION }}";

#[derive(Clone, Copy)]
struct Pin<'a> {
    name: &'static str,
    expected: &'a str,
}

pub(super) fn validate_pin_occurrences(
    node: &Yaml,
    path: &Path,
    tools: &ToolchainPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let pins = pins(tools);
    match node {
        Yaml::Hash(mapping) => {
            for (key, value) in mapping {
                if let Some(name) = key.as_str()
                    && let Some(pin) = pins.iter().find(|pin| pin.name == name)
                    && value.as_str() != Some(pin.expected)
                {
                    diagnostics.push(Diagnostic::at(
                        path,
                        format!("CI `{name}` does not match toolchains.toml `{}`", pin.expected),
                        "restore the CI value from the canonical toolchains.toml pin",
                    ));
                }
                validate_rust_toolchain_input(key, value, path, tools, diagnostics);
                validate_pin_occurrences(value, path, tools, diagnostics);
            }
        }
        Yaml::Array(values) => {
            for value in values {
                validate_pin_occurrences(value, path, tools, diagnostics);
            }
        }
        _ => {}
    }
}

pub(super) fn validate_required_ci_pins(
    workflow: &Hash,
    path: &Path,
    tools: &ToolchainPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let environment = mapping_value(workflow, "env").and_then(Yaml::as_hash);
    for pin in pins(tools) {
        let actual = environment.and_then(|environment| mapping_value(environment, pin.name));
        if actual.is_none() {
            diagnostics.push(Diagnostic::at(
                path,
                format!("root `env.{}` must equal toolchains.toml `{}`", pin.name, pin.expected),
                "define the canonical CI pin at workflow scope using the exact toolchains.toml value",
            ));
        }
    }
}

fn validate_rust_toolchain_input(
    key: &Yaml,
    value: &Yaml,
    path: &Path,
    tools: &ToolchainPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(name) = key.as_str() else { return };
    if !RUST_TOOLCHAIN_INPUTS.contains(&name) {
        return;
    }
    let valid =
        value.as_str().is_some_and(|actual| actual == tools.rust || actual == RUST_ENV_REFERENCE);
    if !valid {
        diagnostics.push(Diagnostic::at(
            path,
            format!("CI `{name}` does not select toolchains.toml Rust `{}`", tools.rust),
            "use the exact Rust pin or the canonical RUST_VERSION environment reference",
        ));
    }
}

fn pins(tools: &ToolchainPolicy) -> [Pin<'_>; 3] {
    [
        Pin { name: RUST_PIN, expected: &tools.rust },
        Pin { name: VERUS_PIN, expected: &tools.verus },
        Pin { name: ARCHIVE_DIGEST_PIN, expected: &tools.archives.linux_x86_64.sha256 },
    ]
}

fn mapping_value<'a>(mapping: &'a Hash, key: &str) -> Option<&'a Yaml> {
    mapping.get(&Yaml::String(key.to_owned()))
}

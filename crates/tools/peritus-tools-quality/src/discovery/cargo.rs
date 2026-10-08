//! Known Cargo project surfaces grounded in manifest and toolchain metadata.

use std::collections::BTreeSet;

use crate::{CheckDefinition, CheckSource};

use super::{
    DiscoveryCause, DiscoveryCoverage, DiscoveryDiagnostic, discovered_definition,
};

pub(super) fn discover(
    manifest: &[u8],
    tool_metadata: &[(String, Vec<u8>)],
    definitions: &mut Vec<CheckDefinition>,
    diagnostics: &mut Vec<DiscoveryDiagnostic>,
) {
    let text = match std::str::from_utf8(manifest) {
        Ok(text) => text,
        Err(_) => {
            diagnostics.push(DiscoveryDiagnostic::new(
                "Cargo.toml",
                DiscoveryCause::InvalidSyntax,
                DiscoveryCoverage::Unavailable,
                "Cargo.toml is not valid UTF-8",
            ));
            return;
        }
    };
    let document = match toml::from_str::<toml::Value>(text) {
        Ok(document) => document,
        Err(error) => {
            diagnostics.push(DiscoveryDiagnostic::new(
                "Cargo.toml",
                DiscoveryCause::InvalidSyntax,
                DiscoveryCoverage::Unavailable,
                format!("Cargo.toml is invalid: {error}"),
            ));
            return;
        }
    };
    let Some(root) = document.as_table() else {
        diagnostics.push(DiscoveryDiagnostic::new(
            "Cargo.toml",
            DiscoveryCause::InvalidSyntax,
            DiscoveryCoverage::Unavailable,
            "Cargo.toml root is not a table",
        ));
        return;
    };
    if !root.get("package").is_some_and(toml::Value::is_table)
        && !root.get("workspace").is_some_and(toml::Value::is_table)
    {
        diagnostics.push(DiscoveryDiagnostic::new(
            "Cargo.toml",
            DiscoveryCause::InvalidSyntax,
            DiscoveryCoverage::Unavailable,
            "Cargo.toml declares neither a package nor a workspace",
        ));
        return;
    }

    add(
        "cargo.check",
        vec!["check", "--all-targets", "--all-features"],
        definitions,
        diagnostics,
    );
    add(
        "cargo.test",
        vec!["test", "--all-targets", "--all-features"],
        definitions,
        diagnostics,
    );

    let components = toolchain_components(tool_metadata, diagnostics);
    if components.contains("clippy") {
        add(
            "cargo.clippy",
            vec!["clippy", "--all-targets", "--all-features", "--", "-D", "warnings"],
            definitions,
            diagnostics,
        );
    }
    if components.contains("rustfmt") {
        add(
            "cargo.fmt",
            vec!["fmt", "--all", "--", "--check"],
            definitions,
            diagnostics,
        );
    }
    let omitted = [
        (!components.contains("clippy")).then_some("clippy"),
        (!components.contains("rustfmt")).then_some("rustfmt"),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if !omitted.is_empty() {
        diagnostics.push(DiscoveryDiagnostic::new(
            "cargo-toolchain",
            DiscoveryCause::ToolMetadataUnavailable,
            DiscoveryCoverage::Partial,
            format!(
                "inspected root toolchain metadata did not establish optional component availability for {}; the corresponding Cargo checks were omitted without executing tools",
                omitted.join(", ")
            ),
        ));
    }
}

fn add(
    name: &str,
    arguments: Vec<&str>,
    definitions: &mut Vec<CheckDefinition>,
    diagnostics: &mut Vec<DiscoveryDiagnostic>,
) {
    match discovered_definition(
        name,
        CheckSource::CargoManifest,
        "cargo",
        arguments.into_iter().map(str::to_owned).collect(),
    ) {
        Ok(definition) => definitions.push(definition),
        Err(error) => diagnostics.push(DiscoveryDiagnostic::new(
            "Cargo.toml",
            DiscoveryCause::UnsupportedSyntax,
            DiscoveryCoverage::Partial,
            error.detail(),
        )),
    }
}

fn toolchain_components(
    metadata: &[(String, Vec<u8>)],
    diagnostics: &mut Vec<DiscoveryDiagnostic>,
) -> BTreeSet<String> {
    let mut components = BTreeSet::new();
    for (filename, bytes) in metadata {
        if filename == "rust-toolchain" {
            match std::str::from_utf8(bytes) {
                Ok(channel) if !channel.trim().is_empty() => {}
                Ok(_) => diagnostics.push(DiscoveryDiagnostic::new(
                    filename,
                    DiscoveryCause::InvalidSyntax,
                    DiscoveryCoverage::Partial,
                    "legacy rust-toolchain metadata is empty",
                )),
                Err(_) => diagnostics.push(DiscoveryDiagnostic::new(
                    filename,
                    DiscoveryCause::InvalidSyntax,
                    DiscoveryCoverage::Partial,
                    "legacy rust-toolchain metadata is not valid UTF-8",
                )),
            }
            continue;
        }
        let text = match std::str::from_utf8(bytes) {
            Ok(text) => text,
            Err(_) => {
                diagnostics.push(DiscoveryDiagnostic::new(
                    filename,
                    DiscoveryCause::InvalidSyntax,
                    DiscoveryCoverage::Partial,
                    "rust-toolchain.toml is not valid UTF-8",
                ));
                continue;
            }
        };
        let document = match toml::from_str::<toml::Value>(text) {
            Ok(document) => document,
            Err(error) => {
                diagnostics.push(DiscoveryDiagnostic::new(
                    filename,
                    DiscoveryCause::InvalidSyntax,
                    DiscoveryCoverage::Partial,
                    format!("rust-toolchain.toml is invalid: {error}"),
                ));
                continue;
            }
        };
        let Some(toolchain) = document.get("toolchain").and_then(toml::Value::as_table) else {
            diagnostics.push(DiscoveryDiagnostic::new(
                filename,
                DiscoveryCause::InvalidSyntax,
                DiscoveryCoverage::Partial,
                "rust-toolchain.toml has no toolchain table",
            ));
            continue;
        };
        let Some(values) = toolchain.get("components") else {
            continue;
        };
        let Some(values) = values.as_array() else {
            diagnostics.push(DiscoveryDiagnostic::new(
                filename,
                DiscoveryCause::InvalidSyntax,
                DiscoveryCoverage::Partial,
                "rust-toolchain.toml components is not an array",
            ));
            continue;
        };
        for value in values {
            if let Some(component) = value.as_str() {
                components.insert(component.to_owned());
            } else {
                diagnostics.push(DiscoveryDiagnostic::new(
                    filename,
                    DiscoveryCause::InvalidSyntax,
                    DiscoveryCoverage::Partial,
                    "rust-toolchain.toml components contains a non-string value",
                ));
            }
        }
    }
    components
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_manifest_is_not_discovery_success() {
        let mut definitions = Vec::new();
        let mut diagnostics = Vec::new();
        discover(b"[package\n", &[], &mut definitions, &mut diagnostics);
        assert!(definitions.is_empty());
        assert_eq!(diagnostics[0].cause(), DiscoveryCause::InvalidSyntax);
    }
}

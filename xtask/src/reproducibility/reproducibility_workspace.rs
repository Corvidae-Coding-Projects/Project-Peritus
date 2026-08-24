use crate::error::{Diagnostic, XtaskError};
use std::fs;
use std::path::Path;

const REVIEWED_POLICY: &str = r#"
[workspace]
resolver = "3"

[workspace.package]
version = "0.0.0"
edition = "2024"
rust-version = "1.97.1"
license = "MIT"
repository = "https://github.com/Corvidae-Coding-Projects/Project-Peritus"

[workspace.dependencies]
vstd = { version = "=0.0.0-2026-08-09-0044", git = "https://github.com/verus-lang/verus.git", rev = "92f466f247f45128c630d1c843fd6e27d2115587" }

[workspace.metadata.peritus]
architecture-policy = "architecture.toml"
rust-toolchain = "1.97.1"
verus-version = "0.2026.08.09.92f466f"
vstd-revision = "92f466f247f45128c630d1c843fd6e27d2115587"

[workspace.lints.rust]
future_incompatible = { level = "deny", priority = -1 }
missing_docs = "deny"
nonstandard_style = { level = "deny", priority = -1 }
rust_2018_idioms = { level = "deny", priority = -1 }
rust_2024_compatibility = { level = "deny", priority = -1 }
unexpected_cfgs = { level = "deny", check-cfg = [
    "cfg(verus_keep_ghost)",
    "cfg(verus_keep_ghost_body)",
    "cfg(verus_only)",
    "cfg(verus_verify_core)",
] }
unsafe_code = "deny"
unused_lifetimes = "deny"
unused_qualifications = "deny"

[workspace.lints.clippy]
all = { level = "warn", priority = -1 }
cargo = { level = "warn", priority = -1 }
nursery = { level = "warn", priority = -1 }
pedantic = { level = "warn", priority = -1 }
multiple_crate_versions = "allow"

[workspace.lints.rustdoc]
bare_urls = "deny"
broken_intra_doc_links = "deny"
private_intra_doc_links = "deny"

[profile.dev]
incremental = false
overflow-checks = true

[profile.test]
incremental = false
overflow-checks = true

[profile.release]
codegen-units = 1
incremental = false
lto = "thin"
overflow-checks = true
panic = "abort"
strip = "symbols"
"#;

pub(super) fn validate(root: &Path, diagnostics: &mut Vec<Diagnostic>) -> Result<(), XtaskError> {
    let path = root.join("Cargo.toml");
    let contents =
        fs::read_to_string(&path).map_err(|error| XtaskError::io("read", &path, error))?;
    validate_contents(&contents, &path, diagnostics)
}

pub(super) fn validate_contents(
    contents: &str,
    path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let actual: toml::Value =
        toml::from_str(contents).map_err(|error| XtaskError::parse_policy(path, error))?;
    let expected: toml::Value =
        toml::from_str(REVIEWED_POLICY).map_err(|error| XtaskError::parse_policy(path, error))?;
    for (keys, description) in [
        (&["workspace", "resolver"][..], "workspace resolver"),
        (&["workspace", "package"][..], "workspace package metadata"),
        (&["workspace", "metadata", "peritus"][..], "workspace Peritus policy metadata"),
        (&["workspace", "dependencies", "vstd"][..], "workspace vstd dependency pin"),
        (&["workspace", "lints"][..], "workspace Rust, Clippy, and rustdoc lint policy"),
        (&["profile", "dev"][..], "development profile"),
        (&["profile", "test"][..], "test profile"),
        (&["profile", "release"][..], "release profile"),
    ] {
        if lookup(&actual, keys) != lookup(&expected, keys) {
            diagnostics.push(Diagnostic::at(
                path,
                format!("{description} differs from the complete reviewed A0 contract"),
                format!("restore the exact reviewed `{}` value and keys", keys.join(".")),
            ));
        }
    }
    Ok(())
}

fn lookup<'a>(value: &'a toml::Value, keys: &[&str]) -> Option<&'a toml::Value> {
    keys.iter().try_fold(value, |current, key| current.get(*key))
}

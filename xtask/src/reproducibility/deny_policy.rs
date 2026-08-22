use crate::error::{Diagnostic, XtaskError};
use std::fs;
use std::path::Path;

const REVIEWED_POLICY: &str = r#"
[graph]
targets = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
    "aarch64-pc-windows-msvc",
]
all-features = true

[advisories]
unmaintained = "workspace"
unsound = "all"
ignore = []

[licenses]
confidence-threshold = 0.93
allow = ["Apache-2.0", "MIT", "Unicode-3.0", "Zlib"]
exceptions = []

[licenses.private]
ignore = false
registries = []

[bans]
multiple-versions = "deny"
wildcards = "deny"
highlight = "all"
workspace-default-features = "allow"
external-default-features = "allow"
allow = []
deny = []
skip = [
    { crate = "syn@2.0.119", reason = "Pinned vstd procedural macros require syn 2 while pinned Serde derive requires syn 3; both are locked, build-time-only parser dependencies." },
]
skip-tree = []

[bans.std-replacements]
scope = "workspace"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
allow-git = ["https://github.com/verus-lang/verus.git"]
"#;

pub(super) fn validate(root: &Path, diagnostics: &mut Vec<Diagnostic>) -> Result<(), XtaskError> {
    let path = root.join("deny.toml");
    let contents =
        fs::read_to_string(&path).map_err(|error| XtaskError::io("read", &path, error))?;
    validate_contents(&contents, &path, diagnostics)
}

fn validate_contents(
    contents: &str,
    path: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let actual: toml::Value =
        toml::from_str(contents).map_err(|error| XtaskError::parse_policy(path, error))?;
    let expected: toml::Value =
        toml::from_str(REVIEWED_POLICY).map_err(|error| XtaskError::parse_policy(path, error))?;
    if actual != expected {
        diagnostics.push(Diagnostic::at(
            path,
            "cargo-deny policy differs from the exact reviewed foundation contract",
            "restore strict advisory/license/source/bans policy and only the version-specific syn 2 exception required by pinned vstd",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{REVIEWED_POLICY, validate_contents};
    use std::path::Path;

    #[test]
    fn exact_policy_accepts_comments_and_key_reordering() {
        let altered = REVIEWED_POLICY.replace("[graph]", "# reviewed\n[graph]");
        let mut diagnostics = Vec::new();
        validate_contents(&altered, Path::new("deny.toml"), &mut diagnostics)
            .expect("valid TOML must parse");
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn broad_or_unreasoned_exceptions_fail_closed() {
        for altered in [
            REVIEWED_POLICY
                .replace("multiple-versions = \"deny\"", "multiple-versions = \"allow\""),
            REVIEWED_POLICY.replace("syn@2.0.119", "syn"),
            REVIEWED_POLICY.replace("skip-tree = []", "skip-tree = [\"vstd\"]"),
        ] {
            let mut diagnostics = Vec::new();
            validate_contents(&altered, Path::new("deny.toml"), &mut diagnostics)
                .expect("valid altered TOML must parse");
            assert!(!diagnostics.is_empty(), "accepted weakened cargo-deny policy");
        }
    }
}

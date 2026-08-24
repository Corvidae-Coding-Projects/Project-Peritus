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
allow = ["Apache-2.0", "BSD-3-Clause", "ISC", "MIT", "MIT-0", "Unicode-3.0", "Zlib"]
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
    { crate = "bitflags@1.3.2", reason = "Pinned portable-pty 0.9 requires bitflags 1 for its Windows backend while current platform and state dependencies use bitflags 2; both are locked implementation-only types." },
    { crate = "block-buffer@0.10.4", reason = "Pinned keyring 4.1.6 reaches the Secret Service crypto stack through sha2 0.10 while Peritus uses sha2 0.11; this locked implementation-only buffer type does not cross a Peritus API." },
    { crate = "cfg_aliases@0.1.1", reason = "Pinned portable-pty 0.9 requires nix 0.28 and its cfg_aliases 0.1 build helper while the process owner uses current nix 0.31 and cfg_aliases 0.2; both helpers are locked and build-time-only." },
    { crate = "cpufeatures@0.2.17", reason = "Pinned keyring 4.1.6 reaches the Secret Service crypto stack through sha2 0.10 while Peritus uses sha2 0.11; this locked implementation-only CPU feature detector does not cross a Peritus API." },
    { crate = "crypto-common@0.1.7", reason = "Pinned keyring 4.1.6 reaches the Secret Service crypto stack through sha2 0.10 while Peritus uses sha2 0.11; this locked implementation-only crypto trait version does not cross a Peritus API." },
    { crate = "digest@0.10.7", reason = "Pinned keyring 4.1.6 reaches the Secret Service crypto stack through sha2 0.10 while Peritus uses sha2 0.11; this locked implementation-only digest trait version does not cross a Peritus API." },
    { crate = "getrandom@0.2.17", reason = "Pinned keyring 4.1.6 reaches Secret Service 5.1, which retains getrandom 0.2 while current workspace utilities use getrandom 0.4; neither locked version crosses a Peritus API." },
    { crate = "nix@0.28.0", reason = "Pinned portable-pty 0.9 owns its private nix 0.28 PTY implementation while process-wrap and Peritus tree control share nix 0.31; neither version crosses the public C2 API." },
    { crate = "sha2@0.10.9", reason = "Pinned keyring 4.1.6 uses Secret Service 5.1 and its private sha2 0.10 crypto implementation while Peritus uses sha2 0.11; both versions are locked and do not share public types." },
    { crate = "syn@2.0.119", reason = "Pinned vstd procedural macros require syn 2 while pinned Serde derive requires syn 3; both are locked, build-time-only parser dependencies." },
    { crate = "thiserror@1.0.69", reason = "Pinned portable-pty 0.9 retains thiserror 1 while pinned Landlock 0.4 uses thiserror 2; both are private error implementations and do not cross Peritus APIs." },
    { crate = "thiserror-impl@1.0.69", reason = "Pinned portable-pty 0.9 retains thiserror 1 while pinned Landlock 0.4 uses thiserror 2; both derive implementations are locked build-time dependencies." },
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
            "restore strict advisory/license/source/bans policy and only the exact reviewed version-specific exceptions required by pinned dependencies",
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
            REVIEWED_POLICY.replace("nix@0.28.0", "nix"),
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

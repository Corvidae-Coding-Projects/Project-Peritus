use super::workflow_command_policy::{config_is_exact, load};
use super::workflow_commands::parse_script;
use crate::error::Diagnostic;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

const CONFIG: &str = r#"
[alias]
xtask = "run --locked --package xtask --"

[build]
incremental = false

[net]
git-fetch-with-cli = true
retry = 2
"#;

#[test]
fn cargo_alias_wrapper_source_and_env_overrides_are_rejected() {
    for extra in [
        "\n[alias]\nverus = \"!./fake-cargo-verus\"\n",
        "\n[alias]\ndeny = \"!true\"\n",
        "\n[build]\nrustc-wrapper = \"./wrapper\"\n",
        "\n[source.crates-io]\nreplace-with = \"attacker\"\n",
        "\n[env]\nPATH = \"./attacker\"\n",
    ] {
        let merged = if extra.starts_with("\n[alias]") {
            CONFIG.replace("[alias]", extra.trim_start())
        } else if extra.starts_with("\n[build]") {
            CONFIG.replace("[build]", extra.trim_start())
        } else {
            format!("{CONFIG}{extra}")
        };
        let value: toml::Value = toml::from_str(&merged).expect("test config must parse");
        assert!(!config_is_exact(&value), "override unexpectedly accepted: {extra}");
    }
    let reviewed: toml::Value = toml::from_str(CONFIG).expect("reviewed config must parse");
    assert!(config_is_exact(&reviewed));
}

#[test]
fn nested_same_named_target_directory_config_is_discovered() {
    let fixture = Fixture::new();
    fixture.write(".cargo/config.toml", CONFIG);
    fixture.write("xtask/target/.cargo/config.toml", "[alias]\nverus = \"!true\"\n");
    let diagnostics = fixture.load();
    assert_message(&diagnostics, "nested or legacy Cargo configuration");
}

#[test]
fn mixed_case_cargo_config_aliases_are_rejected_on_every_host() {
    for alternate in ["xtask/.Cargo/Config.toml", ".Cargo/config", ".cargo/Config.toml"] {
        let fixture = Fixture::new();
        fixture.write(".cargo/config.toml", CONFIG);
        fixture.write(alternate, "[alias]\nverus = \"!true\"\n");
        assert_message(&fixture.load(), "nested or legacy Cargo configuration");
    }
}

#[test]
fn xtask_alias_drift_is_rejected_while_the_canonical_gate_is_direct() {
    let drifted = CONFIG.replace("run --locked --package xtask --", "test --locked --workspace --");
    let value: toml::Value = toml::from_str(&drifted).expect("test config must parse");
    assert!(!config_is_exact(&value));
    assert!(
        parse_script("cargo run --locked --package xtask -- all").exact_cargo_command(&[
            "run",
            "--locked",
            "--package",
            "xtask",
            "--",
            "all",
        ]),
        "the canonical gate must use Cargo's built-in run command rather than alias.xtask"
    );
}

#[cfg(unix)]
#[test]
fn symlinked_root_config_is_rejected() {
    use std::os::unix::fs::symlink;
    let fixture = Fixture::new();
    fixture.write("reviewed.toml", CONFIG);
    fs::create_dir_all(fixture.path().join(".cargo")).expect("cargo directory must be creatable");
    symlink(fixture.path().join("reviewed.toml"), fixture.path().join(".cargo/config.toml"))
        .expect("config symlink must be creatable");
    let diagnostics = fixture.load();
    assert_message(&diagnostics, "reached through a symlink");
}

fn assert_message(diagnostics: &[Diagnostic], expected: &str) {
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message().contains(expected)),
        "expected `{expected}`, got {diagnostics:?}"
    );
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "peritus-repro-config-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).expect("fixture root must be creatable");
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        fs::create_dir_all(path.parent().expect("fixture path must have parent"))
            .expect("fixture directory must be creatable");
        fs::write(path, contents).expect("fixture file must be writable");
    }

    fn load(&self) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        load(&self.root, &mut diagnostics).expect("fixture scan must be readable");
        diagnostics
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).expect("fixture root must be removable");
    }
}

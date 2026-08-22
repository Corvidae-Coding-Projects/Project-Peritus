use super::policy_file;
use super::verus_commands::CANONICAL_VERUS_ARGS;
use super::workflow_actionlint;
use super::workflow_commands::{CommandPolicy, parse_script};
use crate::error::{Diagnostic, XtaskError};
use std::fs;
use std::path::Path;

const REVIEWED_CONFIG: &str = r#"
[alias]
xtask = "run --locked --package xtask --"

[build]
incremental = false

[net]
git-fetch-with-cli = true
retry = 2
"#;
const AUDITED_EXECUTABLES: [&str; 8] =
    ["actionlint", "cargo", "curl", "mkdir", "printf", "set", "sha256sum", "unzip"];

pub(super) fn load(
    root: &Path,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<CommandPolicy, XtaskError> {
    let relative = Path::new(".cargo/config.toml");
    let contents = policy_file::read_regular(
        root,
        relative,
        "root Cargo configuration is missing, non-regular, or reached through a symlink",
        "root Cargo configuration",
        "restore .cargo/config.toml as a checked-in regular file with no symlink components",
        diagnostics,
    );
    reject_nested_configs(root, diagnostics)?;
    let Some(contents) = contents else {
        return Ok(CommandPolicy::new(false));
    };
    let path = root.join(relative);
    let config: toml::Value =
        toml::from_str(&contents).map_err(|error| XtaskError::parse_policy(&path, error))?;
    let valid = config_is_exact(&config);
    if !valid {
        diagnostics.push(Diagnostic::at(
            ".cargo/config.toml",
            "Cargo configuration is not the complete exact reviewed A0 configuration",
            "retain only the locked xtask alias, incremental=false, and reviewed network settings; aliases, wrappers, sources, tools, and env overrides are forbidden",
        ));
    }
    Ok(CommandPolicy::new(valid))
}

pub(super) fn config_is_exact(config: &toml::Value) -> bool {
    toml::from_str::<toml::Value>(REVIEWED_CONFIG).is_ok_and(|expected| *config == expected)
}

fn reject_nested_configs(root: &Path, diagnostics: &mut Vec<Diagnostic>) -> Result<(), XtaskError> {
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| XtaskError::io("read directory", &directory, error))?
        {
            let entry =
                entry.map_err(|error| XtaskError::io("read directory entry", &directory, error))?;
            let path = entry.path();
            let file_type =
                entry.file_type().map_err(|error| XtaskError::io("inspect", &path, error))?;
            if file_type.is_symlink() {
                diagnostics.push(Diagnostic::at(
                    path.strip_prefix(root).unwrap_or(&path),
                    "symlinked repository entry prevents complete Cargo-config discovery",
                    "remove the symlink so nested Cargo configuration cannot be hidden",
                ));
                continue;
            }
            if file_type.is_dir() {
                let name = entry.file_name();
                let root_exclusion = directory == root
                    && [
                        ".git",
                        ".crosslink",
                        ".agents",
                        ".claude",
                        ".codex",
                        ".design",
                        ".worktrees",
                        "reference-repos",
                        "target",
                        "target-verus",
                    ]
                    .iter()
                    .any(|ignored| name == *ignored);
                if !root_exclusion {
                    directories.push(path);
                }
                continue;
            }
            let nested = path
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.eq_ignore_ascii_case(".cargo"))
                && path.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
                    name.eq_ignore_ascii_case("config") || name.eq_ignore_ascii_case("config.toml")
                })
                && path != root.join(".cargo/config.toml");
            if nested {
                diagnostics.push(Diagnostic::at(
                    path.strip_prefix(root).unwrap_or(&path),
                    "nested or legacy Cargo configuration can override the reviewed root policy",
                    "remove every Cargo config except the exact root .cargo/config.toml",
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn validate(
    script: &str,
    path: &Path,
    location: &str,
    policy: CommandPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) {
    validate_with_mode(script, path, location, policy, false, diagnostics);
}

pub(super) fn validate_just(
    script: &str,
    path: &Path,
    location: &str,
    policy: CommandPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) {
    validate_with_mode(script, path, location, policy, true, diagnostics);
}

fn validate_with_mode(
    script: &str,
    path: &Path,
    location: &str,
    policy: CommandPolicy,
    allow_exact_docs_assignment: bool,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let parsed = parse_script(script);
    let actionlint_install = workflow_actionlint::is_reviewed_install(&parsed);
    if !parsed.is_failure_propagating()
        && !parsed.is_reviewed_archive_install()
        && !actionlint_install
        && !parsed.is_reviewed_config_preflight()
    {
        diagnostics.push(Diagnostic::at(
            path,
            format!("`{location}` uses shell behavior outside the failure-propagating command model"),
            "use direct commands with no eval, nested shell, backgrounding, failure masking, or control flow",
        ));
    }
    let exact_docs_assignment = allow_exact_docs_assignment && parsed.exact_docs_command();
    for command in parsed.commands() {
        if command.has_leading_assignments() && !exact_docs_assignment {
            diagnostics.push(Diagnostic::at(
                path,
                format!("`{location}` uses forbidden leading environment assignments"),
                "invoke Cargo directly; only the exact RUSTDOCFLAGS=-D warnings Just docs command is permitted",
            ));
        }
        if let Some(executable) = command.executable_word()
            && !AUDITED_EXECUTABLES.contains(&executable)
            && !(actionlint_install && executable == "tar")
        {
            diagnostics.push(Diagnostic::at(
                path,
                format!(
                    "`{location}` invokes unaudited executable `{executable}` outside the checked command model"
                ),
                "use a direct canonical Cargo operation or the exact reviewed Verus archive-install commands; local scripts, paths, runners, and arbitrary binaries are not inspected",
            ));
        }
        if command.is_xtask() {
            if !policy.permits_xtask() {
                diagnostics.push(Diagnostic::at(
                    path,
                    format!("`{location}` invokes cargo xtask without the exact locked alias"),
                    "restore .cargo/config.toml alias.xtask to the reviewed locked command",
                ));
            }
        } else if command.is_dependency_resolving() && !command.has_locked_input() {
            diagnostics.push(Diagnostic::at(
                path,
                format!(
                    "`{location}` runs `{}` without --locked before Cargo's `--` boundary",
                    command.render()
                ),
                "add --locked to the Cargo invocation before `--` so dependency drift is rejected",
            ));
        }
        if command.is_verus()
            && !CANONICAL_VERUS_ARGS.iter().any(|expected| command.is_exact_cargo(expected))
        {
            if command.has_argument("--no-solver-version-check") {
                diagnostics.push(Diagnostic::at(
                    path,
                    format!("`{location}` disables solver-version enforcement"),
                    "remove --no-solver-version-check and use the pinned solver",
                ));
            }
            diagnostics.push(Diagnostic::at(
                path,
                format!("`{location}` uses a non-canonical cargo-verus invocation"),
                "use an exact full-workspace TCB-aware command or its paired V/H no-cheating command with pinned solver-resource flags",
            ));
        }
    }
}

//! Canonical Rust reliability package and host-observed activation state.

use serde_json::Value;

#[derive(Default)]
pub struct Activation {
    active: bool,
    refreshes: u32,
    drifting: bool,
}

impl Activation {
    pub(crate) fn task(&mut self, task: &str) {
        self.active |= task.split(['\n', ';']).flat_map(|line| line.split(". ")).any(|clause| {
            let words: Vec<_> = clause
                .split_whitespace()
                .map(|word| {
                    word.trim_matches(|ch: char| {
                        !(ch.is_alphanumeric() || matches!(ch, '_' | '.' | '/' | '-' | '\''))
                    })
                })
                .collect();
            let lower: Vec<_> = words.iter().map(|word| word.to_ascii_lowercase()).collect();
            let action = lower.iter().any(|word| {
                matches!(
                    word.as_str(),
                    "fix"
                        | "implement"
                        | "read"
                        | "write"
                        | "update"
                        | "review"
                        | "debug"
                        | "refactor"
                        | "build"
                        | "inspect"
                        | "change"
                        | "create"
                        | "edit"
                        | "resume"
                        | "continue"
                        | "compile"
                        | "use"
                )
            });
            words.iter().enumerate().any(|(index, word)| {
                let before = &lower[index.saturating_sub(5)..index];
                let after = &lower[index + 1..(index + 4).min(words.len())];
                // Negation applies to this target, not an unrelated exclusion later in the
                // sentence ("Implement a Rust CLI, but do not run unit tests").
                let excluded = before.iter().any(|word| {
                    matches!(
                        word.as_str(),
                        "not"
                            | "no"
                            | "never"
                            | "avoid"
                            | "unrelated"
                            | "preserve"
                            | "leave"
                            | "instead"
                            | "don't"
                            | "dont"
                    )
                }) || before.iter().rev().take(3).any(|word| word == "without")
                    || after.iter().any(|word| {
                        matches!(word.as_str(), "unrelated" | "untouched" | "unchanged")
                    });
                if excluded {
                    return false;
                }
                if rust_path(word.trim_end_matches('.')) {
                    return action;
                }
                lower[index] == "rust"
                    && (before.iter().any(|word| {
                        matches!(
                            word.as_str(),
                            "write"
                                | "implement"
                                | "refactor"
                                | "fix"
                                | "review"
                                | "debug"
                                | "in"
                                | "using"
                        )
                    }) || after.iter().any(|word| {
                        matches!(
                            word.as_str(),
                            "code"
                                | "cli"
                                | "crate"
                                | "project"
                                | "program"
                                | "implementation"
                                | "function"
                                | "module"
                                | "browser"
                        )
                    }))
            })
        });
    }

    pub(crate) fn observe(
        &mut self,
        name: &str,
        arguments: &Value,
        value: &Value,
        succeeded: bool,
        mutation: bool,
    ) {
        if !succeeded {
            return;
        }
        if matches!(
            name,
            "workspace_read" | "workspace_search" | "workspace_write" | "workspace_patch"
        ) {
            self.active |= arguments.get("path").and_then(Value::as_str).is_some_and(rust_path);
        }
        if matches!(name, "run_command" | "command_start") {
            self.active |=
                arguments.get("program").and_then(Value::as_str).is_some_and(|program| {
                    std::path::Path::new(program)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| {
                            matches!(
                                name,
                                "cargo"
                                    | "rustc"
                                    | "rustfmt"
                                    | "cargo.exe"
                                    | "rustc.exe"
                                    | "rustfmt.exe"
                            )
                        })
                });
        }
        // Listing alone does not activate the skill: a mixed repository may host non-Rust work.
        // Tool response text is evidence, never a source of system instructions.
        if name == "skill_read"
            && arguments.get("skill").and_then(Value::as_str) == Some("rust-reliability")
        {
            self.active = true;
        }
        if mutation && value.get("changed").and_then(Value::as_bool) != Some(false) {
            self.drifting = false;
        }
    }

    pub(crate) const fn refresh(&mut self) -> Option<&'static str> {
        if !self.active {
            return None;
        }
        self.refreshes = self.refreshes.saturating_add(1);
        self.drifting = true;
        Some(
            "Rust reliability refresh: the skill remains in the current system policy. Reuse unchanged evidence; identify the concrete ownership, cancellation, state or lifecycle contract relevant to this task, then take the next implementation or real-interface observation step. Read a bundled reference through skill_read only when it answers an unresolved question. This reminder is advisory, creates no acceptance gate, and does not authorize forbidden commands.",
        )
    }

    pub(crate) fn guidance(&self) -> Option<String> {
        self.active.then(|| format!(
            "ACTIVE BUNDLED SKILL [rust-reliability-v1]\nrefresh_count={}; drift_reminder={}\nThe host activated Rust reliability from Rust task or tool activity. Apply it within your current role and permissions; read-only roles may inspect and report but cannot edit, execute its helper, or invoke processes. User restrictions on checks and tests take precedence: the optional rust_checks.py helper must not run when its build or Clippy commands are forbidden. This skill introduces no automatic gates or new completion obligations. Its relative resources are available using skill_read with skill=rust-reliability and the exact path shown in the skill. If a permitted helper is needed, read its complete source, materialize it only in an authorized disposable location, and run it only within existing command authority. Keep reasoning private.\nRust core: preserve ownership and error contracts; own and cancel async work, avoid locks across await, validate persistence/recovery boundaries, minimize unsafe and explain its invariants. Use real behavioral evidence for changes. Read only the relevant rust-reliability resource sections through skill_read when a concrete question requires them; do not reread unchanged guidance after a role change or compaction.",
            self.refreshes, self.drifting,
        ))
    }
}

fn rust_path(path: &str) -> bool {
    let path = std::path::Path::new(path);
    path.extension().is_some_and(|extension| extension == "rs")
        || path.file_name().is_some_and(|name| {
            matches!(
                name.to_str(),
                Some("Cargo.toml" | "Cargo.lock" | "rust-toolchain" | "rust-toolchain.toml")
            )
        })
}

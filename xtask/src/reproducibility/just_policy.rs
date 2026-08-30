use super::verus_commands::{
    VERUS_STRICT_BUILD_ARGS, VERUS_STRICT_VERIFY_ARGS, VERUS_WORKSPACE_BUILD_ARGS,
    VERUS_WORKSPACE_VERIFY_ARGS,
};
use super::workflow_command_contracts::WORKSPACE_TEST_ARGS;
use super::workflow_command_policy;
use super::workflow_command_policy::CommandPolicy;
use super::workflow_commands::parse_script;
use crate::error::{Diagnostic, XtaskError};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Default)]
struct Recipe {
    dependencies: Vec<String>,
    commands: Vec<String>,
    ignored_failure: bool,
}

pub(super) fn validate(
    root: &Path,
    policy: CommandPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), XtaskError> {
    let path = root.join("justfile");
    let contents =
        fs::read_to_string(&path).map_err(|error| XtaskError::io("read", &path, error))?;
    validate_contents(&contents, policy, diagnostics);
    Ok(())
}

pub(super) fn validate_contents(
    contents: &str,
    policy: CommandPolicy,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let recipes = parse_recipes(contents, diagnostics);
    let path = Path::new("justfile");
    require_inventory(&recipes, diagnostics);
    for (name, recipe) in &recipes {
        if recipe.ignored_failure {
            diagnostics.push(Diagnostic::at(
                path,
                format!("recipe `{name}` uses Just's `-` failure-ignoring sigil"),
                "remove `-` so every gate command propagates failure",
            ));
        }
        for command in &recipe.commands {
            workflow_command_policy::validate_just(
                command,
                path,
                &format!("recipe `{name}` command"),
                policy,
                diagnostics,
            );
        }
    }

    validate_recipe_contract(&recipes, diagnostics);
}

fn validate_recipe_contract(recipes: &BTreeMap<String, Recipe>, diagnostics: &mut Vec<Diagnostic>) {
    require_dependencies(recipes, "default", &["check"], diagnostics);
    require_dependencies(
        recipes,
        "check",
        &["fmt", "build", "test", "doc-test", "clippy", "docs"],
        diagnostics,
    );
    require_dependencies(
        recipes,
        "gate-a",
        &["check", "ordinary-api", "deny", "toolchain", "verus-verify", "verus-build"],
        diagnostics,
    );
    require_exact(
        recipes,
        "check",
        &["run", "--locked", "--package", "xtask", "--", "all"],
        diagnostics,
    );
    for (name, operation) in [
        ("fmt", &["fmt", "--all", "--", "--check"][..]),
        ("build", &["build", "--workspace", "--all-targets", "--all-features", "--locked"][..]),
        ("test", WORKSPACE_TEST_ARGS),
        ("doc-test", &["test", "--doc", "--workspace", "--all-features", "--locked"][..]),
        (
            "clippy",
            &[
                "clippy",
                "--workspace",
                "--all-targets",
                "--all-features",
                "--locked",
                "--",
                "-D",
                "warnings",
            ][..],
        ),
        (
            "architecture",
            &["run", "--locked", "--package", "xtask", "--", "architecture-check"][..],
        ),
        (
            "source-layout",
            &["run", "--locked", "--package", "xtask", "--", "source-layout-check"][..],
        ),
        (
            "reproducibility",
            &["run", "--locked", "--package", "xtask", "--", "reproducibility-check"][..],
        ),
        (
            "ordinary-api",
            &["run", "--locked", "--package", "xtask", "--", "ordinary-api-check"][..],
        ),
        ("trust", &["run", "--locked", "--package", "xtask", "--", "verify-trust"][..]),
        ("licenses", &["deny", "--locked", "check", "bans", "licenses", "sources"][..]),
    ] {
        require_dependencies(recipes, name, &[], diagnostics);
        require_exact(recipes, name, operation, diagnostics);
    }
    require_dependencies(recipes, "docs", &[], diagnostics);
    require_exact_docs(recipes, diagnostics);
    require_exact(recipes, "deny", &["deny", "--locked", "check"], diagnostics);
    require_exact(
        recipes,
        "toolchain",
        &["run", "--locked", "--package", "xtask", "--", "toolchain-check"],
        diagnostics,
    );
    require_exact_sequence(
        recipes,
        "verus-verify",
        &[VERUS_WORKSPACE_VERIFY_ARGS, VERUS_STRICT_VERIFY_ARGS],
        diagnostics,
    );
    for name in ["deny", "ordinary-api", "toolchain", "verus-verify", "verus-build"] {
        require_dependencies(recipes, name, &[], diagnostics);
    }
    require_no_commands(recipes, "default", diagnostics);
    require_exact_sequence(
        recipes,
        "verus-build",
        &[VERUS_WORKSPACE_BUILD_ARGS, VERUS_STRICT_BUILD_ARGS],
        diagnostics,
    );
    if recipes.get("gate-a").is_some_and(|recipe| !recipe.commands.is_empty()) {
        diagnostics.push(Diagnostic::at(
            "justfile",
            "recipe `gate-a` contains commands outside its reviewed dependency closure",
            "make gate-a depend exactly on check, ordinary-api, deny, toolchain, verus-verify, and verus-build",
        ));
    }
}

fn parse_recipes(contents: &str, diagnostics: &mut Vec<Diagnostic>) -> BTreeMap<String, Recipe> {
    let mut recipes = BTreeMap::<String, Recipe>::new();
    let mut current: Option<String> = None;
    let mut continued = String::new();
    for line in contents.lines() {
        if !line.starts_with([' ', '\t']) {
            flush_command(&mut recipes, current.as_deref(), &mut continued, diagnostics);
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                current = None;
                continue;
            }
            current = if let Some((name, dependencies)) = parse_header(line) {
                if recipes
                    .insert(name.clone(), Recipe { dependencies, ..Recipe::default() })
                    .is_some()
                {
                    diagnostics.push(Diagnostic::at(
                        "justfile",
                        format!("recipe `{name}` is defined more than once"),
                        "retain one exact canonical definition for every reviewed recipe",
                    ));
                }
                Some(name)
            } else {
                diagnostics.push(Diagnostic::at(
                    "justfile",
                    format!("unsupported top-level Just directive `{trimmed}`"),
                    "remove variables, imports, modules, settings, attributes, and custom shell configuration",
                ));
                None
            };
            continue;
        }
        let Some(name) = current.as_deref() else {
            if !line.trim().is_empty() {
                diagnostics.push(Diagnostic::at(
                    "justfile",
                    "found an indented command outside a reviewed recipe",
                    "place commands only in the exact canonical recipe inventory",
                ));
            }
            continue;
        };
        let trimmed = line.trim();
        if trimmed.starts_with("#!") {
            diagnostics.push(Diagnostic::at(
                "justfile",
                format!("recipe `{name}` uses forbidden Just shebang/script mode"),
                "remove the shebang and retain the direct canonical recipe command",
            ));
            continue;
        }
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let (ignored, command) =
            if continued.is_empty() { strip_sigils(trimmed) } else { (false, trimmed) };
        if ignored && let Some(recipe) = recipes.get_mut(name) {
            recipe.ignored_failure = true;
        }
        continued.push_str(command.trim_end_matches('\\').trim_end());
        if command.ends_with('\\') {
            continued.push(' ');
        } else {
            flush_command(&mut recipes, Some(name), &mut continued, diagnostics);
        }
    }
    flush_command(&mut recipes, current.as_deref(), &mut continued, diagnostics);
    recipes
}

fn parse_header(line: &str) -> Option<(String, Vec<String>)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') || line.contains(":=") {
        return None;
    }
    let (head, tail) = line.split_once(':')?;
    let name = head.trim();
    if name.is_empty()
        || !name.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return None;
    }
    let dependencies = tail.split_whitespace().map(ToOwned::to_owned).collect();
    Some((name.to_owned(), dependencies))
}

fn require_inventory(recipes: &BTreeMap<String, Recipe>, diagnostics: &mut Vec<Diagnostic>) {
    let expected = [
        "architecture",
        "build",
        "check",
        "clippy",
        "default",
        "deny",
        "doc-test",
        "docs",
        "fmt",
        "gate-a",
        "licenses",
        "ordinary-api",
        "reproducibility",
        "source-layout",
        "test",
        "toolchain",
        "trust",
        "verus-build",
        "verus-verify",
    ];
    if recipes.keys().map(String::as_str).ne(expected) {
        diagnostics.push(Diagnostic::at(
            "justfile",
            "Just recipe inventory differs from the exact reviewed foundation interface",
            "restore every canonical recipe and remove unreviewed recipes or directives",
        ));
    }
}

fn require_no_commands(
    recipes: &BTreeMap<String, Recipe>,
    name: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if !recipes.get(name).is_some_and(|recipe| recipe.commands.is_empty()) {
        diagnostics.push(Diagnostic::at(
            "justfile",
            format!("recipe `{name}` must only compose its reviewed dependencies"),
            "remove direct commands from this composition recipe",
        ));
    }
}

fn strip_sigils(mut command: &str) -> (bool, &str) {
    let mut ignored = false;
    while let Some(sigil) = command.chars().next().filter(|value| matches!(value, '@' | '-')) {
        ignored |= sigil == '-';
        command = &command[sigil.len_utf8()..];
    }
    (ignored, command.trim_start())
}

fn flush_command(
    recipes: &mut BTreeMap<String, Recipe>,
    current: Option<&str>,
    command: &mut String,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if command.is_empty() {
        return;
    }
    if let Some(recipe) = current.and_then(|name| recipes.get_mut(name)) {
        recipe.commands.push(std::mem::take(command));
    } else {
        diagnostics.push(Diagnostic::at(
            "justfile",
            "found a command outside a named recipe",
            "place every checked command under an explicit recipe",
        ));
        command.clear();
    }
}

fn require_dependencies(
    recipes: &BTreeMap<String, Recipe>,
    name: &str,
    expected: &[&str],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let actual = recipes.get(name).map(|recipe| recipe.dependencies.iter().map(String::as_str));
    if !actual.is_some_and(|actual| actual.eq(expected.iter().copied())) {
        diagnostics.push(Diagnostic::at(
            "justfile",
            format!("recipe `{name}` does not have the exact reviewed gate dependencies"),
            format!("set `{name}` dependencies to: {}", expected.join(" ")),
        ));
    }
}

fn require_exact(
    recipes: &BTreeMap<String, Recipe>,
    name: &str,
    expected: &[&str],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let valid = recipes.get(name).is_some_and(|recipe| {
        recipe.commands.len() == 1
            && parse_script(&recipe.commands[0]).exact_cargo_command(expected)
            && !recipe.ignored_failure
    });
    if !valid {
        diagnostics.push(Diagnostic::at(
            "justfile",
            format!("recipe `{name}` does not contain its exact canonical Cargo operation"),
            format!(
                "restore `cargo {}` as the recipe's sole failure-propagating command",
                expected.join(" ")
            ),
        ));
    }
}

fn require_exact_sequence(
    recipes: &BTreeMap<String, Recipe>,
    name: &str,
    expected: &[&[&str]],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let valid = recipes.get(name).is_some_and(|recipe| {
        !recipe.ignored_failure
            && recipe.commands.len() == expected.len()
            && recipe
                .commands
                .iter()
                .zip(expected)
                .all(|(command, expected)| parse_script(command).exact_cargo_command(expected))
    });
    if !valid {
        diagnostics.push(Diagnostic::at(
            "justfile",
            format!("recipe `{name}` does not contain its exact TCB-aware command sequence"),
            "restore the full-workspace command followed by the exact V/H no-cheating command",
        ));
    }
}

fn require_exact_docs(recipes: &BTreeMap<String, Recipe>, diagnostics: &mut Vec<Diagnostic>) {
    let valid = recipes.get("docs").is_some_and(|recipe| {
        recipe.commands.len() == 1
            && parse_script(&recipe.commands[0]).exact_docs_command()
            && !recipe.ignored_failure
    });
    if !valid {
        diagnostics.push(Diagnostic::at(
            "justfile",
            "recipe `docs` does not contain its exact canonical Cargo operation",
            "restore `RUSTDOCFLAGS='-D warnings' cargo doc --workspace --all-features --no-deps --locked` as the sole command",
        ));
    }
}

use super::super::lexer::{TypeBinding, type_bindings};
use super::module_symbol;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Resolves the supported inherent self-type paths to a unique nominal definition.
/// Only explicit imports/re-exports and standard module files are followed. Glob-only
/// imports, type aliases, cycles and ambiguous declarations fail closed instead of guessing.
pub(super) fn resolve(
    owning_crate: &str,
    source: &Path,
    contents: &str,
    inline_modules: &[String],
    owner: &[String],
) -> Option<String> {
    let file_module = module_symbol(owning_crate, source)?;
    let mut module: Vec<_> = file_module.split("::").map(str::to_owned).collect();
    module.extend_from_slice(inline_modules);
    let first = owner.first()?;
    let target = if matches!(first.as_str(), "crate" | "self" | "super") {
        absolute(&module, owner)?
    } else {
        match type_bindings(contents, inline_modules, first).as_slice() {
            [TypeBinding::Nominal] if owner.len() == 1 => {
                module.push(first.clone());
                return Some(module.join("::"));
            }
            [TypeBinding::Import(import)] => {
                let mut path = import.clone();
                path.extend_from_slice(&owner[1..]);
                absolute(&module, &path)?
            }
            _ => return None,
        }
    };
    let source_root = source.ancestors().find(|path| {
        matches!(path.file_name().and_then(|name| name.to_str()), Some("src" | "tests"))
    })?;
    nominal_owner(source_root, &module[0], target)
}

fn nominal_owner(root: &Path, crate_name: &str, mut target: Vec<String>) -> Option<String> {
    let mut visited = BTreeSet::new();
    while visited.insert(target.clone()) {
        let (name, modules) = target.split_last()?;
        if modules.first().map(String::as_str) != Some(crate_name) {
            return None;
        }
        let (text, inline) = definition_source(root, &modules[1..])?;
        match type_bindings(&text, &inline, name).as_slice() {
            [TypeBinding::Nominal] => return Some(target.join("::")),
            [TypeBinding::Import(import)] => target = absolute(modules, import)?,
            _ => return None,
        }
    }
    None
}

fn absolute(module: &[String], path: &[String]) -> Option<Vec<String>> {
    let mut prefix = module.to_vec();
    let mut cursor = 0;
    match path.first()?.as_str() {
        "crate" => {
            prefix.truncate(1);
            cursor = 1;
        }
        "self" => cursor = 1,
        "super" => {
            while path.get(cursor).is_some_and(|segment| segment == "super") {
                if prefix.len() <= 1 {
                    return None;
                }
                prefix.pop();
                cursor += 1;
            }
        }
        _ => {}
    }
    if cursor == path.len() {
        return None;
    }
    if path[cursor..].iter().any(|segment| matches!(segment.as_str(), "crate" | "self" | "super")) {
        return None;
    }
    prefix.extend_from_slice(&path[cursor..]);
    Some(prefix)
}

fn definition_source(root: &Path, modules: &[String]) -> Option<(String, Vec<String>)> {
    for boundary in (0..=modules.len()).rev() {
        let candidates = if boundary == 0 {
            [root.join("lib.rs"), root.join("main.rs")]
        } else {
            let base =
                modules[..boundary].iter().fold(PathBuf::from(root), |path, name| path.join(name));
            [base.with_extension("rs"), base.join("mod.rs")]
        };
        let files: Vec<_> = candidates.iter().filter(|path| path.is_file()).collect();
        match files.as_slice() {
            [file] => return Some((fs::read_to_string(file).ok()?, modules[boundary..].to_vec())),
            [] => {}
            _ => return None,
        }
    }
    None
}

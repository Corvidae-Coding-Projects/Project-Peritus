//! Bounded zero-argument public Just recipe discovery.

use std::collections::BTreeSet;

use crate::{CheckDefinition, CheckSource, QualityError, QualityErrorKind};

use super::discovered_definition;

const MAX_LINES: usize = 16_384;
const MAX_RECIPES: usize = 1_024;

pub(super) fn discover(
    filename: &str,
    bytes: &[u8],
    definitions: &mut Vec<CheckDefinition>,
) -> Result<(), QualityError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| QualityError::new(QualityErrorKind::Parser, "Justfile is not valid UTF-8"))?;
    if text.lines().count() > MAX_LINES {
        return Err(parser_error("Justfile exceeds its line bound"));
    }
    let mut names = BTreeSet::new();
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty()
            || trimmed.starts_with('#')
            || trimmed.starts_with('@')
            || trimmed.len() != line.len()
        {
            continue;
        }
        if let Some(name) = zero_argument_recipe(trimmed) {
            names.insert(name.to_owned());
            if names.len() > MAX_RECIPES {
                return Err(parser_error("Justfile recipe count exceeds its bound"));
            }
        }
    }
    for name in names {
        definitions.push(discovered_definition(
            &format!("just.{name}"),
            CheckSource::JustfileRecipe(format!("{filename}:{name}")),
            "just",
            vec![name],
        )?);
    }
    Ok(())
}

fn zero_argument_recipe(line: &str) -> Option<&str> {
    let (head, _) = line.split_once(':')?;
    if head.is_empty()
        || head.starts_with('_')
        || head.contains(char::is_whitespace)
        || !head.bytes().all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return None;
    }
    Some(head)
}

fn parser_error(detail: &'static str) -> QualityError {
    QualityError::new(QualityErrorKind::Parser, detail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_only_public_zero_argument_recipes() {
        let mut definitions = Vec::new();
        discover(
            "Justfile",
            b"check:\n  cargo check\nwith-arg value:\n  echo {{value}}\n_private:\n  true\n",
            &mut definitions,
        )
        .expect("discovery");
        let names: Vec<_> = definitions.iter().map(CheckDefinition::gate_name).collect();
        assert_eq!(names, ["just.check"]);
    }
}

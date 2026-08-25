use super::verus_commands::{VERUS_STRICT_BUILD_ARGS, VERUS_STRICT_VERIFY_ARGS};
use crate::error::Diagnostic;
use crate::model::ArchitecturePolicy;
use std::collections::BTreeSet;
use std::path::Path;

pub(super) fn validate(policy: &ArchitecturePolicy, diagnostics: &mut Vec<Diagnostic>) {
    let required: BTreeSet<_> = policy
        .packages
        .iter()
        .filter(|package| matches!(package.verification_class.as_str(), "V" | "H"))
        .map(|package| package.name.as_str())
        .collect();
    let verify = package_arguments(VERUS_STRICT_VERIFY_ARGS);
    let build = package_arguments(VERUS_STRICT_BUILD_ARGS);
    let expected: Vec<_> = required.iter().copied().collect();

    if verify != expected || build != expected {
        diagnostics.push(Diagnostic::at(
            Path::new("justfile"),
            "V/H no-cheating command coverage differs from the architecture registry",
            format!(
                "configure one exact no-cheating verify and release-build command for every V/H package; required: {}",
                required.iter().copied().collect::<Vec<_>>().join(", ")
            ),
        ));
    }
}

fn package_arguments<'arguments>(arguments: &'arguments [&'arguments str]) -> Vec<&'arguments str> {
    arguments.windows(2).filter_map(|pair| (pair[0] == "--package").then_some(pair[1])).collect()
}

#[cfg(test)]
mod tests;

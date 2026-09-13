use crate::api_contract::Mode;
use crate::error::XtaskError;
use crate::model::ToolchainPolicy;
use crate::trust::RegisteredProofSymbol;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Deserialize)]
pub(super) struct CompilerReport {
    #[serde(rename = "func-details")]
    functions: BTreeMap<String, serde_json::Value>,
    #[serde(rename = "verification-results")]
    result: VerificationResult,
    #[serde(rename = "times-ms")]
    times: Times,
    verus: Build,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[allow(
    clippy::struct_excessive_bools,
    reason = "these fields mirror the pinned Verus JSON schema"
)]
struct VerificationResult {
    encountered_error: bool,
    encountered_vir_error: bool,
    success: bool,
    errors: u64,
    is_verifying_entire_crate: bool,
}

#[derive(Debug, Deserialize)]
struct Build {
    version: String,
    commit: String,
    toolchain: String,
}

#[derive(Debug, Deserialize)]
struct Times {
    smt: Smt,
}

#[derive(Debug, Deserialize)]
struct Smt {
    #[serde(rename = "smt-run-module-times")]
    modules: Vec<Module>,
}

#[derive(Debug, Deserialize)]
struct Module {
    #[serde(rename = "function-breakdown")]
    functions: Vec<Query>,
}

#[derive(Debug, Deserialize)]
struct Query {
    function: String,
    #[serde(rename = "mode:")]
    mode: String,
    success: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Scope {
    package: String,
    root_targets: BTreeSet<String>,
    pub(super) registered_symbols: usize,
    pub(super) queried_functions: BTreeMap<String, String>,
    selected_functions: BTreeSet<String>,
    has_solver_queries: bool,
}

/// Reads a complete previously generated observation, without claiming freshness or discharge.
pub(crate) fn observation(bytes: &[u8], package: &str) -> Result<serde_json::Value, XtaskError> {
    let scope: Scope = serde_json::from_slice(bytes)
        .map_err(|error| invalid(format!("invalid observed compiler scope: {error}")))?;
    let prefixes: Vec<_> =
        scope.root_targets.iter().map(|target| format!("{}::", target.replace('-', "_"))).collect();
    if scope.package != package
        || scope.root_targets.is_empty()
        || scope.root_targets.iter().any(String::is_empty)
        || scope.registered_symbols > scope.selected_functions.len()
        || scope.has_solver_queries == scope.queried_functions.is_empty()
        || scope
            .selected_functions
            .iter()
            .any(|function| !prefixes.iter().any(|prefix| function.starts_with(prefix)))
        || scope.queried_functions.iter().any(|(function, mode)| {
            !scope.selected_functions.contains(function)
                || !prefixes.iter().any(|prefix| function.starts_with(prefix))
                || !matches!(mode.as_str(), "exec" | "proof" | "spec")
        })
    {
        return Err(invalid(
            "observed compiler scope is inconsistent or belongs to another package",
        ));
    }
    serde_json::to_value(scope)
        .map_err(|error| invalid(format!("cannot render observed compiler scope: {error}")))
}

/// Decodes concatenated compiler objects and the dependency summaries emitted by Cargo-Verus.
/// Unknown text, malformed objects, failed dependency summaries, and absent reports fail closed.
pub(super) fn parse(bytes: &[u8]) -> Result<Vec<CompilerReport>, XtaskError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|error| invalid(format!("compiler output is not UTF-8: {error}")))?;
    let mut remaining = text.trim_start();
    let mut reports = Vec::new();
    while !remaining.is_empty() {
        if remaining.starts_with('{') {
            let mut decoder =
                serde_json::Deserializer::from_str(remaining).into_iter::<CompilerReport>();
            let report = decoder
                .next()
                .ok_or_else(|| invalid("missing compiler object"))?
                .map_err(|error| invalid(format!("invalid compiler object: {error}")))?;
            let offset = decoder.byte_offset();
            reports.push(report);
            remaining = remaining[offset..].trim_start();
        } else {
            let (line, rest) = remaining.split_once('\n').unwrap_or((remaining, ""));
            let count = line
                .strip_prefix("verification results:: ")
                .and_then(|line| line.strip_suffix(" verified, 0 errors"));
            if count.is_none_or(|value| value.parse::<u64>().is_err()) {
                return Err(invalid("unrecognized or unsuccessful compiler output"));
            }
            remaining = rest.trim_start();
        }
    }
    if reports.is_empty() {
        return Err(invalid("no fresh compiler selection report was returned"));
    }
    Ok(reports)
}

pub(super) fn check(
    reports: &[CompilerReport],
    package: &str,
    root_targets: &[&str],
    expected: &[RegisteredProofSymbol],
    tools: &ToolchainPolicy,
) -> Result<Scope, XtaskError> {
    if reports.is_empty() || root_targets.is_empty() {
        return Err(invalid("a fresh root invocation and its target names are required"));
    }
    let prefixes: BTreeSet<_> =
        root_targets.iter().map(|target| format!("{}::", target.replace('-', "_"))).collect();
    let mut selected_functions = BTreeSet::new();
    let mut queried_functions = BTreeMap::new();
    for report in reports {
        validate_result(report, tools)?;
        selected_functions.extend(
            report
                .functions
                .keys()
                .filter(|name| prefixes.iter().any(|prefix| name.starts_with(prefix)))
                .cloned(),
        );
        for query in report.times.smt.modules.iter().flat_map(|module| &module.functions) {
            if !query.success || !matches!(query.mode.as_str(), "exec" | "proof" | "spec") {
                return Err(invalid(format!("failed or unrecognized query `{}`", query.function)));
            }
            if prefixes.iter().any(|prefix| query.function.starts_with(prefix)) {
                if !report.functions.contains_key(&query.function) {
                    return Err(invalid("query is absent from its compiler selection inventory"));
                }
                if let Some(previous) =
                    queried_functions.insert(query.function.clone(), query.mode.clone())
                    && previous != query.mode
                {
                    return Err(invalid("conflicting modes for the same selected function"));
                }
            } else {
                return Err(invalid(format!(
                    "query `{}` belongs to a different root invocation",
                    query.function
                )));
            }
        }
    }
    let mut registered = BTreeSet::new();
    for evidence in expected {
        if evidence.owner != package {
            continue;
        }
        if !selected_functions.contains(&evidence.symbol) {
            return Err(invalid(format!(
                "obligation `{obligation}` cites `{symbol}`, which Verus did not select",
                obligation = evidence.obligation,
                symbol = evidence.symbol
            )));
        }
        let required_query = match evidence.mode {
            Mode::Exec => Some("exec"),
            Mode::Proof => Some("proof"),
            Mode::Spec => None,
        };
        if let Some(required_query) = required_query
            && queried_functions.get(&evidence.symbol).map(String::as_str) != Some(required_query)
        {
            return Err(invalid(format!(
                "obligation `{}` cites `{}` as {required_query} evidence, but the successful compiler query is absent or has another mode",
                evidence.obligation, evidence.symbol
            )));
        }
        registered.insert(&evidence.symbol);
    }
    Ok(Scope {
        package: package.to_owned(),
        root_targets: root_targets.iter().map(|name| (*name).to_owned()).collect(),
        registered_symbols: registered.len(),
        has_solver_queries: !queried_functions.is_empty(),
        queried_functions,
        selected_functions,
    })
}

fn validate_result(report: &CompilerReport, tools: &ToolchainPolicy) -> Result<(), XtaskError> {
    let result = &report.result;
    if !result.success
        || result.encountered_error
        || result.encountered_vir_error
        || result.errors != 0
        || !result.is_verifying_entire_crate
    {
        return Err(invalid("compiler result is failed, partial, or incomplete"));
    }
    if report.verus.version != tools.verus
        || report.verus.commit != tools.vstd_revision
        || !report.verus.toolchain.starts_with(&format!("{}-", tools.rust))
    {
        return Err(invalid("compiler result does not match the pinned Verus and Rust identities"));
    }
    Ok(())
}

fn invalid(message: impl AsRef<str>) -> XtaskError {
    XtaskError::metadata(format!("Verus proof scope: {}", message.as_ref()))
}

#[cfg(test)]
mod tests;

//! Strict command grammar for `HarnessBench` campaign reports.

use std::{collections::BTreeMap, ffi::OsString, path::PathBuf};

use super::model::{IdentityPolicy, ReportRequest};
use crate::BenchmarkError;

const USAGE: &str = "peritus-harnessbench-report --campaign-dir PATH --task-catalog PATH --output PATH --pin-file PATH --expected-tasks COUNT --campaign-label LABEL --identity-policy allow-legacy|require-native";
const OPTIONS: &[&str] = &[
    "--campaign-dir",
    "--task-catalog",
    "--output",
    "--pin-file",
    "--expected-tasks",
    "--campaign-label",
    "--identity-policy",
];

pub(super) fn parse<I>(arguments: I) -> Result<ReportRequest, BenchmarkError>
where
    I: IntoIterator<Item = OsString>,
{
    let mut values = arguments.into_iter();
    let _program = values.next();
    let values = values.collect::<Vec<_>>();
    if !values.len().is_multiple_of(2) {
        return Err(invalid(format!("options require values; usage: {USAGE}")));
    }
    let mut fields = BTreeMap::new();
    for pair in values.chunks_exact(2) {
        let name = text(pair.first().cloned(), "option name is not UTF-8")?;
        let value = text(pair.get(1).cloned(), "option value is not UTF-8")?;
        if !OPTIONS.contains(&name.as_str()) {
            return Err(invalid(format!("unknown option {name:?}; usage: {USAGE}")));
        }
        if fields.insert(name.clone(), value).is_some() {
            return Err(invalid(format!("duplicate option {name:?}")));
        }
    }
    let expected_tasks = required(&mut fields, "--expected-tasks")?
        .parse::<usize>()
        .map_err(|_| invalid("--expected-tasks must be a positive integer"))?;
    let identity_policy = match required(&mut fields, "--identity-policy")?.as_str() {
        "allow-legacy" => IdentityPolicy::AllowLegacy,
        "require-native" => IdentityPolicy::RequireNative,
        _ => return Err(invalid("--identity-policy must be allow-legacy or require-native")),
    };
    Ok(ReportRequest {
        campaign_directory: PathBuf::from(required(&mut fields, "--campaign-dir")?),
        task_catalog: PathBuf::from(required(&mut fields, "--task-catalog")?),
        output: PathBuf::from(required(&mut fields, "--output")?),
        pin_file: PathBuf::from(required(&mut fields, "--pin-file")?),
        expected_tasks,
        campaign_label: required(&mut fields, "--campaign-label")?,
        identity_policy,
    })
}

fn required(
    fields: &mut BTreeMap<String, String>,
    name: &'static str,
) -> Result<String, BenchmarkError> {
    let value =
        fields.remove(name).ok_or_else(|| invalid(format!("missing {name}; usage: {USAGE}")))?;
    if value.trim().is_empty() {
        return Err(invalid(format!("{name} must not be empty")));
    }
    Ok(value)
}

fn text(value: Option<OsString>, detail: &'static str) -> Result<String, BenchmarkError> {
    value.ok_or_else(|| invalid(detail))?.into_string().map_err(|_| invalid(detail))
}

fn invalid(detail: impl Into<String>) -> BenchmarkError {
    BenchmarkError::Arguments(detail.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn complete() -> Vec<OsString> {
        [
            "report",
            "--campaign-dir",
            "/state/campaign",
            "--task-catalog",
            "/state/tasks",
            "--output",
            "/state/report.json",
            "--pin-file",
            "/repo/pin.toml",
            "--expected-tasks",
            "106",
            "--campaign-label",
            "diagnostic-baseline",
            "--identity-policy",
            "allow-legacy",
        ]
        .into_iter()
        .map(OsString::from)
        .collect()
    }

    #[test]
    fn parses_complete_request() {
        let request = parse(complete()).expect("request");
        assert_eq!(request.expected_tasks, 106);
        assert_eq!(request.campaign_label, "diagnostic-baseline");
        assert_eq!(request.identity_policy, IdentityPolicy::AllowLegacy);
    }

    #[test]
    fn rejects_unknown_and_duplicate_options() {
        let mut duplicate = complete();
        duplicate.extend([OsString::from("--expected-tasks"), OsString::from("1")]);
        assert!(parse(duplicate).is_err());
        let mut unknown = complete();
        unknown.extend([OsString::from("--answer"), OsString::from("42")]);
        assert!(parse(unknown).is_err());
    }
}

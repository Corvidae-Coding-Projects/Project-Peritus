//! Strict command grammar for Terminal-Bench campaign reports.

use std::{collections::BTreeMap, ffi::OsString, path::PathBuf};

use super::model::{CampaignMode, IdentityPolicy, ReportRequest};
use crate::BenchmarkError;

const USAGE: &str = "peritus-terminalbench-report --job-dir PATH --output PATH --pin-file PATH --expected-trials COUNT --mode snapshot|final --campaign-label LABEL --identity-policy allow-legacy|require-native --agent-sha256 DIGEST";
const OPTIONS: &[&str] = &[
    "--job-dir",
    "--output",
    "--pin-file",
    "--expected-trials",
    "--mode",
    "--campaign-label",
    "--identity-policy",
    "--agent-sha256",
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

    let expected_trials = required(&mut fields, "--expected-trials")?
        .parse::<usize>()
        .map_err(|_| invalid("--expected-trials must be a positive integer"))?;
    let mode = match required(&mut fields, "--mode")?.as_str() {
        "snapshot" => CampaignMode::Snapshot,
        "final" => CampaignMode::Final,
        _ => return Err(invalid("--mode must be snapshot or final")),
    };
    let identity_policy = match required(&mut fields, "--identity-policy")?.as_str() {
        "allow-legacy" => IdentityPolicy::AllowLegacy,
        "require-native" => IdentityPolicy::RequireNative,
        _ => return Err(invalid("--identity-policy must be allow-legacy or require-native")),
    };
    Ok(ReportRequest {
        job_directory: PathBuf::from(required(&mut fields, "--job-dir")?),
        output: PathBuf::from(required(&mut fields, "--output")?),
        pin_file: PathBuf::from(required(&mut fields, "--pin-file")?),
        expected_trials,
        mode,
        campaign_label: required(&mut fields, "--campaign-label")?,
        identity_policy,
        agent_sha256: required(&mut fields, "--agent-sha256")?,
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

    fn complete(mode: &str) -> Vec<OsString> {
        [
            "report",
            "--job-dir",
            "/state/job",
            "--output",
            "/state/report.json",
            "--pin-file",
            "/repo/pin.toml",
            "--expected-trials",
            "445",
            "--mode",
            mode,
            "--campaign-label",
            "frozen-baseline",
            "--identity-policy",
            "require-native",
            "--agent-sha256",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        ]
        .into_iter()
        .map(OsString::from)
        .collect()
    }

    #[test]
    fn parses_complete_final_request() {
        let request = parse(complete("final")).expect("request");
        assert_eq!(request.mode, CampaignMode::Final);
        assert_eq!(request.expected_trials, 445);
        assert_eq!(request.campaign_label, "frozen-baseline");
        assert_eq!(request.identity_policy, IdentityPolicy::RequireNative);
    }

    #[test]
    fn rejects_unknown_mode_and_duplicate_options() {
        assert!(parse(complete("maybe")).is_err());
        let mut duplicate = complete("snapshot");
        duplicate.extend([OsString::from("--mode"), OsString::from("final")]);
        assert!(parse(duplicate).is_err());
    }
}

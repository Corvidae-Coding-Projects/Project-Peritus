//! Strict benchmark-agent command grammar.

use std::{collections::BTreeMap, ffi::OsString, path::PathBuf, time::Duration};

use crate::BenchmarkError;
const HARNESSBENCH_USAGE: &str = "peritus-benchmark-agent harnessbench --workspace PATH --sandbox PATH --prompt-file PATH --session-id ID --task-id ID --model-id ID --adapter-schema-version VERSION --suite-revision REVISION";
const TERMINALBENCH_USAGE: &str = "peritus-benchmark-agent terminalbench --workspace PATH --evidence-dir PATH --prompt-file PATH --session-id ID --task-id ID --model-id ID --max-elapsed-seconds SECONDS --adapter-schema-version VERSION --suite-revision REVISION";

pub enum Command {
    HarnessBench(HarnessBenchInput),
    TerminalBench(TerminalBenchInput),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HarnessBenchInput {
    pub workspace: PathBuf,
    pub sandbox: PathBuf,
    pub prompt_file: PathBuf,
    pub session_id: String,
    pub task_id: String,
    pub model_id: String,
    pub adapter_schema_version: u32,
    pub suite_revision: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalBenchInput {
    pub workspace: PathBuf,
    pub evidence_dir: PathBuf,
    pub prompt_file: PathBuf,
    pub session_id: String,
    pub task_id: String,
    pub model_id: String,
    pub max_elapsed: Duration,
    pub adapter_schema_version: u32,
    pub suite_revision: String,
}

impl Command {
    pub fn parse<I>(arguments: I) -> Result<Self, BenchmarkError>
    where
        I: IntoIterator<Item = OsString>,
    {
        let mut values = arguments.into_iter();
        let _program = values.next();
        let command = text(values.next(), "missing command")?;
        let remaining = values.collect::<Vec<_>>();
        match command.as_str() {
            "harnessbench" => parse_harnessbench(&remaining).map(Self::HarnessBench),
            "terminalbench" => parse_terminalbench(&remaining).map(Self::TerminalBench),
            _ => Err(invalid(format!(
                "unknown command {command:?}; usage: {HARNESSBENCH_USAGE}; or: {TERMINALBENCH_USAGE}"
            ))),
        }
    }
}

fn parse_harnessbench(values: &[OsString]) -> Result<HarnessBenchInput, BenchmarkError> {
    let mut fields = parse_fields(
        values,
        &[
            "--workspace",
            "--sandbox",
            "--prompt-file",
            "--session-id",
            "--task-id",
            "--model-id",
            "--adapter-schema-version",
            "--suite-revision",
        ],
        HARNESSBENCH_USAGE,
    )?;
    Ok(HarnessBenchInput {
        workspace: required_path(&mut fields, "--workspace", HARNESSBENCH_USAGE)?,
        sandbox: required_path(&mut fields, "--sandbox", HARNESSBENCH_USAGE)?,
        prompt_file: required_path(&mut fields, "--prompt-file", HARNESSBENCH_USAGE)?,
        session_id: required(&mut fields, "--session-id", HARNESSBENCH_USAGE)?,
        task_id: required(&mut fields, "--task-id", HARNESSBENCH_USAGE)?,
        model_id: required(&mut fields, "--model-id", HARNESSBENCH_USAGE)?,
        adapter_schema_version: required_u32(
            &mut fields,
            "--adapter-schema-version",
            HARNESSBENCH_USAGE,
        )?,
        suite_revision: required(&mut fields, "--suite-revision", HARNESSBENCH_USAGE)?,
    })
}

fn parse_terminalbench(values: &[OsString]) -> Result<TerminalBenchInput, BenchmarkError> {
    let mut fields = parse_fields(
        values,
        &[
            "--workspace",
            "--evidence-dir",
            "--prompt-file",
            "--session-id",
            "--task-id",
            "--model-id",
            "--max-elapsed-seconds",
            "--adapter-schema-version",
            "--suite-revision",
        ],
        TERMINALBENCH_USAGE,
    )?;
    Ok(TerminalBenchInput {
        workspace: required_path(&mut fields, "--workspace", TERMINALBENCH_USAGE)?,
        evidence_dir: required_path(&mut fields, "--evidence-dir", TERMINALBENCH_USAGE)?,
        prompt_file: required_path(&mut fields, "--prompt-file", TERMINALBENCH_USAGE)?,
        session_id: required(&mut fields, "--session-id", TERMINALBENCH_USAGE)?,
        task_id: required(&mut fields, "--task-id", TERMINALBENCH_USAGE)?,
        model_id: required(&mut fields, "--model-id", TERMINALBENCH_USAGE)?,
        max_elapsed: required_duration(&mut fields, "--max-elapsed-seconds", TERMINALBENCH_USAGE)?,
        adapter_schema_version: required_u32(
            &mut fields,
            "--adapter-schema-version",
            TERMINALBENCH_USAGE,
        )?,
        suite_revision: required(&mut fields, "--suite-revision", TERMINALBENCH_USAGE)?,
    })
}

fn parse_fields(
    values: &[OsString],
    allowed: &[&str],
    usage: &str,
) -> Result<BTreeMap<String, String>, BenchmarkError> {
    if !values.len().is_multiple_of(2) {
        return Err(invalid(format!("options require values; usage: {usage}")));
    }
    let mut fields = BTreeMap::new();
    for pair in values.chunks_exact(2) {
        let name = text(pair.first().cloned(), "option name is not UTF-8")?;
        let value = text(pair.get(1).cloned(), "option value is not UTF-8")?;
        if !allowed.contains(&name.as_str()) {
            return Err(invalid(format!("unknown option {name:?}; usage: {usage}")));
        }
        if fields.insert(name.clone(), value).is_some() {
            return Err(invalid(format!("duplicate option {name:?}")));
        }
    }
    Ok(fields)
}

fn required_path(
    fields: &mut BTreeMap<String, String>,
    name: &'static str,
    usage: &str,
) -> Result<PathBuf, BenchmarkError> {
    required(fields, name, usage).map(PathBuf::from)
}

fn required_duration(
    fields: &mut BTreeMap<String, String>,
    name: &'static str,
    usage: &str,
) -> Result<Duration, BenchmarkError> {
    let raw = required(fields, name, usage)?;
    let seconds =
        raw.parse::<u64>().map_err(|_| invalid(format!("{name} must be a positive integer")))?;
    let duration = Duration::from_secs(seconds);
    if duration.is_zero() {
        return Err(invalid(format!("{name} must be a positive integer")));
    }
    Ok(duration)
}

fn required_u32(
    fields: &mut BTreeMap<String, String>,
    name: &'static str,
    usage: &str,
) -> Result<u32, BenchmarkError> {
    required(fields, name, usage)?
        .parse::<u32>()
        .map_err(|_| invalid(format!("{name} must be an unsigned integer")))
}

fn required(
    fields: &mut BTreeMap<String, String>,
    name: &'static str,
    usage: &str,
) -> Result<String, BenchmarkError> {
    let value =
        fields.remove(name).ok_or_else(|| invalid(format!("missing {name}; usage: {usage}")))?;
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

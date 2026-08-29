//! Strict benchmark-agent command grammar.

use std::{collections::BTreeMap, ffi::OsString, path::PathBuf};

use crate::BenchmarkError;

const HARNESSBENCH_USAGE: &str = "peritus-benchmark-agent harnessbench --workspace PATH --sandbox PATH --prompt-file PATH --session-id ID --task-id ID --model-id ID";
const TERMINALBENCH_USAGE: &str = "peritus-benchmark-agent terminalbench --workspace PATH --evidence-dir PATH --prompt-file PATH --session-id ID --task-id ID --model-id ID";

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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TerminalBenchInput {
    pub workspace: PathBuf,
    pub evidence_dir: PathBuf,
    pub prompt_file: PathBuf,
    pub session_id: String,
    pub task_id: String,
    pub model_id: String,
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
        &["--workspace", "--sandbox", "--prompt-file", "--session-id", "--task-id", "--model-id"],
        HARNESSBENCH_USAGE,
    )?;
    Ok(HarnessBenchInput {
        workspace: required_path(&mut fields, "--workspace", HARNESSBENCH_USAGE)?,
        sandbox: required_path(&mut fields, "--sandbox", HARNESSBENCH_USAGE)?,
        prompt_file: required_path(&mut fields, "--prompt-file", HARNESSBENCH_USAGE)?,
        session_id: required(&mut fields, "--session-id", HARNESSBENCH_USAGE)?,
        task_id: required(&mut fields, "--task-id", HARNESSBENCH_USAGE)?,
        model_id: required(&mut fields, "--model-id", HARNESSBENCH_USAGE)?,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_complete_harnessbench_command() {
        let input = Command::parse(
            [
                "agent",
                "harnessbench",
                "--workspace",
                "/tmp/workspace",
                "--sandbox",
                "/tmp/sandbox",
                "--prompt-file",
                "/tmp/prompt",
                "--session-id",
                "session",
                "--task-id",
                "001-file",
                "--model-id",
                "peritus",
            ]
            .into_iter()
            .map(OsString::from),
        )
        .expect("command");
        let Command::HarnessBench(input) = input else {
            panic!("harnessbench variant");
        };
        assert_eq!(input.task_id, "001-file");
        assert_eq!(input.workspace, PathBuf::from("/tmp/workspace"));
    }

    #[test]
    fn parses_complete_terminalbench_command() {
        let input = Command::parse(
            [
                "agent",
                "terminalbench",
                "--workspace",
                "/app",
                "--evidence-dir",
                "/logs/agent/peritus",
                "--prompt-file",
                "/tmp/instruction.md",
                "--session-id",
                "trial",
                "--task-id",
                "task",
                "--model-id",
                "peritus",
            ]
            .into_iter()
            .map(OsString::from),
        )
        .expect("command");
        let Command::TerminalBench(input) = input else {
            panic!("terminalbench variant");
        };
        assert_eq!(input.workspace, PathBuf::from("/app"));
        assert_eq!(input.evidence_dir, PathBuf::from("/logs/agent/peritus"));
    }

    #[test]
    fn rejects_unknown_and_duplicate_options() {
        for arguments in [
            vec!["agent", "harnessbench", "--surprise", "value"],
            vec!["agent", "harnessbench", "--workspace", "one", "--workspace", "two"],
        ] {
            assert!(Command::parse(arguments.into_iter().map(OsString::from)).is_err());
        }
    }
}

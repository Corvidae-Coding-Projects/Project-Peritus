//! Strict benchmark-agent command grammar.

use std::{collections::BTreeMap, ffi::OsString, path::PathBuf};

use crate::BenchmarkError;

const USAGE: &str = "peritus-benchmark-agent harnessbench --workspace PATH --sandbox PATH --prompt-file PATH --session-id ID --task-id ID --model-id ID";

pub enum Command {
    HarnessBench(HarnessBenchInput),
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
            _ => Err(invalid(format!("unknown command {command:?}; usage: {USAGE}"))),
        }
    }
}

fn parse_harnessbench(values: &[OsString]) -> Result<HarnessBenchInput, BenchmarkError> {
    if !values.len().is_multiple_of(2) {
        return Err(invalid(format!("options require values; usage: {USAGE}")));
    }
    let mut fields = BTreeMap::new();
    for pair in values.chunks_exact(2) {
        let name = text(pair.first().cloned(), "option name is not UTF-8")?;
        let value = text(pair.get(1).cloned(), "option value is not UTF-8")?;
        if !matches!(
            name.as_str(),
            "--workspace"
                | "--sandbox"
                | "--prompt-file"
                | "--session-id"
                | "--task-id"
                | "--model-id"
        ) {
            return Err(invalid(format!("unknown option {name:?}; usage: {USAGE}")));
        }
        if fields.insert(name.clone(), value).is_some() {
            return Err(invalid(format!("duplicate option {name:?}")));
        }
    }
    Ok(HarnessBenchInput {
        workspace: PathBuf::from(required(&mut fields, "--workspace")?),
        sandbox: PathBuf::from(required(&mut fields, "--sandbox")?),
        prompt_file: PathBuf::from(required(&mut fields, "--prompt-file")?),
        session_id: required(&mut fields, "--session-id")?,
        task_id: required(&mut fields, "--task-id")?,
        model_id: required(&mut fields, "--model-id")?,
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
        let Command::HarnessBench(input) = input;
        assert_eq!(input.task_id, "001-file");
        assert_eq!(input.workspace, PathBuf::from("/tmp/workspace"));
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

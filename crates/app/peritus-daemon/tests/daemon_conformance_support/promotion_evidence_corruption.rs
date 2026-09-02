//! Public-binary F0 harness-activation evidence containment qualification.

use std::io::{self, Read};
use std::process::{Command, Stdio};

use super::process::{TestEnvironment, peritusd_executable};

const OUTPUT_BOUND: usize = 4_096;

pub fn corruption_recovery() -> io::Result<()> {
    let environment = TestEnvironment::new()?;
    let staged = run(&environment, "qualify-promotion-evidence-corruption-stage")?;
    let stage = fields(&staged, "peritus-qualification promotion-evidence-corruption-stage ", 6)?;
    let identity = fixed_hex(stage.first().copied(), "evidence_id", 32)?;
    let record = fixed_hex(stage.get(1).copied(), "record_sha256", 64)?;
    let corrupt = fixed_hex(stage.get(2).copied(), "corrupt_bytes_sha256", 64)?;
    let pointer = fixed_hex(stage.get(3).copied(), "pointer_sha256", 64)?;
    let bytes = field(stage.get(4).copied(), "bytes")?;
    if record == corrupt
        || bytes.parse::<u64>().is_err()
        || field(stage.get(5).copied(), "corruption_detected")? != "true"
    {
        return Err(io::Error::other("promotion evidence fault checkpoint differs"));
    }

    let recovered = run(&environment, "qualify-promotion-evidence-corruption-recover")?;
    let recovery =
        fields(&recovered, "peritus-qualification promotion-evidence-corruption-recover ", 11)?;
    if fixed_hex(recovery.first().copied(), "evidence_id", 32)? != identity
        || fixed_hex(recovery.get(1).copied(), "corrupt_bytes_sha256", 64)? != corrupt
        || fixed_hex(recovery.get(2).copied(), "quarantine_sha256", 64)? == corrupt
        || fixed_hex(recovery.get(3).copied(), "pointer_sha256", 64)? != pointer
        || field(recovery.get(4).copied(), "bytes")? != bytes
        || field(recovery.get(5).copied(), "committed_events")? != "16"
        || field(recovery.get(6).copied(), "aggregate_heads")? != "4"
        || field(recovery.get(7).copied(), "journal_verified")? != "true"
        || field(recovery.get(8).copied(), "promotion_verified")? != "true"
        || field(recovery.get(9).copied(), "corruption_detected")? != "true"
        || field(recovery.get(10).copied(), "mutation_admitted")? != "false"
    {
        return Err(io::Error::other("promotion evidence containment facts differ"));
    }
    Ok(())
}

fn run(environment: &TestEnvironment, name: &str) -> io::Result<String> {
    let mut child = Command::new(peritusd_executable()?)
        .arg(name)
        .arg("--config")
        .arg(environment.config_path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let status = TestEnvironment::wait_for_exit(&mut child)?;
    let stdout = read_pipe(child.stdout.take())?;
    let stderr = read_pipe(child.stderr.take())?;
    if !status.success() || !stderr.is_empty() {
        return Err(io::Error::other(format!(
            "promotion evidence qualifier exited with {status}: {stderr}"
        )));
    }
    one_line(&stdout)
}

fn read_pipe(pipe: Option<impl Read>) -> io::Result<String> {
    let mut bytes = Vec::new();
    pipe.ok_or_else(|| io::Error::other("promotion evidence subprocess pipe is unavailable"))?
        .take((OUTPUT_BOUND + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > OUTPUT_BOUND {
        return Err(io::Error::other("promotion evidence output exceeded its bound"));
    }
    String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn one_line(value: &str) -> io::Result<String> {
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.contains(['\r', '\n']) {
        return Err(io::Error::other("promotion evidence output is not one line"));
    }
    Ok(value.to_owned())
}

fn fields<'a>(line: &'a str, prefix: &str, count: usize) -> io::Result<Vec<&'a str>> {
    let values = line
        .strip_prefix(prefix)
        .ok_or_else(|| io::Error::other("promotion evidence observation prefix differs"))?
        .split_ascii_whitespace()
        .collect::<Vec<_>>();
    if values.len() != count {
        return Err(io::Error::other("promotion evidence field count differs"));
    }
    Ok(values)
}

fn fixed_hex<'a>(value: Option<&'a str>, name: &str, length: usize) -> io::Result<&'a str> {
    let value = field(value, name)?;
    if value.len() == length
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(value)
    } else {
        Err(io::Error::other("promotion evidence field is not canonical hexadecimal"))
    }
}

fn field<'a>(value: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let (observed, value) = value
        .and_then(|value| value.split_once('='))
        .ok_or_else(|| io::Error::other("promotion evidence observation field is missing"))?;
    if observed != name || value.is_empty() {
        return Err(io::Error::other("promotion evidence observation field differs"));
    }
    Ok(value)
}

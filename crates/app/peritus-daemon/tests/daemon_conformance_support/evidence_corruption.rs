//! Public-binary acceptance-evidence corruption and containment qualification.

use std::io::{self, Read};
use std::process::{Command, Stdio};

use super::process::{TestEnvironment, peritusd_executable};

const OUTPUT_BOUND: usize = 4_096;

pub fn corruption_recovery() -> io::Result<()> {
    let environment = TestEnvironment::new()?;
    let staged = run(&environment, "qualify-evidence-corruption-stage")?;
    let stage = fields(&staged, "peritus-qualification evidence-corruption-stage ", 6)?;
    let identity = fixed_hex(stage.first().copied(), "evidence_id", 32)?;
    let record = digest_field(stage.get(1).copied(), "record_sha256")?;
    let original = digest_field(stage.get(2).copied(), "original_bytes_sha256")?;
    let corrupt = digest_field(stage.get(3).copied(), "corrupt_bytes_sha256")?;
    let bytes = field(stage.get(4).copied(), "bytes")?;
    if record == corrupt
        || original == corrupt
        || bytes.parse::<u64>().is_err()
        || field(stage.get(5).copied(), "corruption_detected")? != "true"
    {
        return Err(io::Error::other("acceptance evidence fault checkpoint differs"));
    }

    let recovered = run(&environment, "qualify-evidence-corruption-recover")?;
    let recovery = fields(&recovered, "peritus-qualification evidence-corruption-recover ", 9)?;
    if fixed_hex(recovery.first().copied(), "evidence_id", 32)? != identity
        || digest_field(recovery.get(1).copied(), "corrupt_bytes_sha256")? != corrupt
        || digest_field(recovery.get(2).copied(), "quarantine_sha256")? == corrupt
        || field(recovery.get(3).copied(), "bytes")? != bytes
        || field(recovery.get(4).copied(), "committed_events")? != "1"
        || field(recovery.get(5).copied(), "aggregate_heads")? != "1"
        || field(recovery.get(6).copied(), "journal_verified")? != "true"
        || field(recovery.get(7).copied(), "corruption_detected")? != "true"
        || field(recovery.get(8).copied(), "mutation_admitted")? != "false"
    {
        return Err(io::Error::other("acceptance evidence containment facts differ"));
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
            "acceptance evidence qualifier exited with {status}: {stderr}"
        )));
    }
    one_line(&stdout)
}

fn read_pipe(pipe: Option<impl Read>) -> io::Result<String> {
    let mut bytes = Vec::new();
    pipe.ok_or_else(|| io::Error::other("evidence corruption subprocess pipe is unavailable"))?
        .take((OUTPUT_BOUND + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > OUTPUT_BOUND {
        return Err(io::Error::other("evidence corruption output exceeded its bound"));
    }
    String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn one_line(value: &str) -> io::Result<String> {
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.contains(['\r', '\n']) {
        return Err(io::Error::other("evidence corruption output is not one line"));
    }
    Ok(value.to_owned())
}

fn fields<'a>(line: &'a str, prefix: &str, count: usize) -> io::Result<Vec<&'a str>> {
    let values = line
        .strip_prefix(prefix)
        .ok_or_else(|| io::Error::other("evidence corruption observation prefix differs"))?
        .split_ascii_whitespace()
        .collect::<Vec<_>>();
    if values.len() != count {
        return Err(io::Error::other("evidence corruption field count differs"));
    }
    Ok(values)
}

fn digest_field<'a>(value: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    fixed_hex(value, name, 64)
}

fn fixed_hex<'a>(value: Option<&'a str>, name: &str, length: usize) -> io::Result<&'a str> {
    let value = field(value, name)?;
    if value.len() == length
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(value)
    } else {
        Err(io::Error::other("evidence corruption field is not canonical hexadecimal"))
    }
}

fn field<'a>(value: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let (observed, value) = value
        .and_then(|value| value.split_once('='))
        .ok_or_else(|| io::Error::other("evidence corruption observation field is missing"))?;
    if observed != name || value.is_empty() {
        return Err(io::Error::other("evidence corruption observation field differs"));
    }
    Ok(value)
}

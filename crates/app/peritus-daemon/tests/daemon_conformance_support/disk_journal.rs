//! Public-binary journal page-exhaustion and fresh-open qualification.

use std::io::{self, Read};
use std::process::{Command, Stdio};

use super::process::{TestEnvironment, peritusd_executable};

const OUTPUT_BOUND: usize = 4_096;

pub fn journal_append_exhaustion_recovery() -> io::Result<()> {
    let environment = TestEnvironment::new()?;
    let staged = run(&environment, "qualify-disk-journal-append-stage")?;
    let stage = fields(&staged, "peritus-qualification disk-journal-append-stage ")?;
    let request = digest_field(stage.first().copied(), "request_sha256")?;
    let page_count = positive_number(stage.get(1).copied(), "page_count")?;
    let page_size = positive_number(stage.get(2).copied(), "page_size")?;
    let maximum_bytes = positive_number(stage.get(3).copied(), "maximum_bytes")?;
    if maximum_bytes != page_count.saturating_mul(page_size)
        || field(stage.get(4).copied(), "storage_exhausted")? != "true"
        || field(stage.get(5).copied(), "append_absent")? != "true"
    {
        return Err(io::Error::other("journal exhaustion checkpoint differs"));
    }

    let recovered = run(&environment, "qualify-disk-journal-append-recover")?;
    let recovery = fields(&recovered, "peritus-qualification disk-journal-append-recover ")?;
    if digest_field(recovery.first().copied(), "request_sha256")? != request
        || positive_number(recovery.get(1).copied(), "page_count")? != page_count
        || positive_number(recovery.get(2).copied(), "page_size")? != page_size
        || positive_number(recovery.get(3).copied(), "maximum_bytes")? < maximum_bytes
        || field(recovery.get(4).copied(), "committed_events")? != "0"
        || field(recovery.get(5).copied(), "aggregate_heads")? != "0"
        || field(recovery.get(6).copied(), "journal_verified")? != "true"
        || field(recovery.get(7).copied(), "append_absent")? != "true"
    {
        return Err(io::Error::other(format!(
            "fresh-open journal exhaustion facts differ: staged={staged}; recovered={recovered}"
        )));
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
            "journal exhaustion qualifier exited with {status}: {stderr}"
        )));
    }
    one_line(&stdout)
}

fn read_pipe(pipe: Option<impl Read>) -> io::Result<String> {
    let mut bytes = Vec::new();
    pipe.ok_or_else(|| io::Error::other("journal exhaustion subprocess pipe is unavailable"))?
        .take((OUTPUT_BOUND + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > OUTPUT_BOUND {
        return Err(io::Error::other("journal exhaustion output exceeded its bound"));
    }
    String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn one_line(value: &str) -> io::Result<String> {
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.contains(['\r', '\n']) {
        return Err(io::Error::other("journal exhaustion output is not one line"));
    }
    Ok(value.to_owned())
}

fn fields<'a>(line: &'a str, prefix: &str) -> io::Result<Vec<&'a str>> {
    line.strip_prefix(prefix)
        .map(|value| value.split_ascii_whitespace().collect())
        .ok_or_else(|| io::Error::other("journal exhaustion observation prefix differs"))
}

fn digest_field<'a>(value: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let value = field(value, name)?;
    if value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(value)
    } else {
        Err(io::Error::other("journal exhaustion digest is not canonical SHA-256"))
    }
}

fn positive_number(value: Option<&str>, name: &str) -> io::Result<u64> {
    field(value, name)?
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| io::Error::other("journal exhaustion page value is not a positive integer"))
}

fn field<'a>(value: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let (observed, value) = value
        .and_then(|value| value.split_once('='))
        .ok_or_else(|| io::Error::other("journal exhaustion observation field is missing"))?;
    if observed != name || value.is_empty() {
        return Err(io::Error::other("journal exhaustion observation field differs"));
    }
    Ok(value)
}

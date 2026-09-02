//! Public-binary projection corruption and startup repair qualification.

use std::io::{self, Read};
use std::process::{Command, Stdio};

use super::process::{TestEnvironment, peritusd_executable};

const OUTPUT_BOUND: usize = 4_096;

pub fn corruption_recovery() -> io::Result<()> {
    let environment = TestEnvironment::new()?;
    let staged = run(&environment, "qualify-projection-corruption-stage")?;
    let stage = fields(&staged, "peritus-qualification projection-corruption-stage ")?;
    let projection = field(stage.first().copied(), "projection")?;
    let generation = field(stage.get(1).copied(), "generation")?;
    let original = digest_field(stage.get(2).copied(), "original_payload_sha256")?;
    let corrupt = digest_field(stage.get(3).copied(), "corrupt_payload_sha256")?;
    let payload_bytes = field(stage.get(4).copied(), "payload_bytes")?;
    if generation != "1" || original == corrupt || payload_bytes == "0" {
        return Err(io::Error::other("projection corruption checkpoint differs"));
    }
    if field(stage.get(5).copied(), "corrupted")? != "true" {
        return Err(io::Error::other("projection payload was not corrupted"));
    }

    let recovered = run(&environment, "qualify-projection-corruption-recover")?;
    let recovery = fields(&recovered, "peritus-qualification projection-corruption-recover ")?;
    if field(recovery.first().copied(), "projection")? != projection
        || field(recovery.get(1).copied(), "previous_generation")? != generation
        || field(recovery.get(2).copied(), "repaired_generation")? != "2"
        || field(recovery.get(3).copied(), "corrupt_payload_sha256")? != corrupt
        || field(recovery.get(4).copied(), "repaired_payload_sha256")? != original
        || field(recovery.get(5).copied(), "payload_bytes")? != payload_bytes
        || field(recovery.get(6).copied(), "generation_count")? != "2"
        || field(recovery.get(7).copied(), "event_count")? != "0"
        || field(recovery.get(8).copied(), "aggregate_heads")? != "0"
        || field(recovery.get(9).copied(), "payload_valid")? != "true"
        || field(recovery.get(10).copied(), "reusable")? != "true"
    {
        return Err(io::Error::other("projection startup repair facts differ"));
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
            "projection qualifier exited with {status}: {stderr}"
        )));
    }
    one_line(&stdout)
}

fn read_pipe(pipe: Option<impl Read>) -> io::Result<String> {
    let mut bytes = Vec::new();
    pipe.ok_or_else(|| io::Error::other("projection subprocess pipe is unavailable"))?
        .take((OUTPUT_BOUND + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > OUTPUT_BOUND {
        return Err(io::Error::other("projection subprocess output exceeded its bound"));
    }
    String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn one_line(value: &str) -> io::Result<String> {
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.contains(['\r', '\n']) {
        return Err(io::Error::other("projection output is not one line"));
    }
    Ok(value.to_owned())
}

fn fields<'a>(line: &'a str, prefix: &str) -> io::Result<Vec<&'a str>> {
    line.strip_prefix(prefix)
        .map(|value| value.split_ascii_whitespace().collect())
        .ok_or_else(|| io::Error::other("projection observation prefix differs"))
}

fn digest_field<'a>(value: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let value = field(value, name)?;
    if value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(value)
    } else {
        Err(io::Error::other("projection digest is not canonical SHA-256"))
    }
}

fn field<'a>(value: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let (observed, value) = value
        .and_then(|value| value.split_once('='))
        .ok_or_else(|| io::Error::other("projection observation field is missing"))?;
    if observed != name || value.is_empty() {
        return Err(io::Error::other("projection observation field differs"));
    }
    Ok(value)
}

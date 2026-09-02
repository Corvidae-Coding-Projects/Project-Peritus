//! Public-binary journal corruption and fail-closed startup qualification.

use std::io::{self, Read};
use std::process::{Command, Stdio};

use super::process::{TestEnvironment, peritusd_executable};

const OUTPUT_BOUND: usize = 4_096;

pub fn corruption_recovery() -> io::Result<()> {
    let environment = TestEnvironment::new()?;
    let staged = run(&environment, "qualify-journal-corruption-stage")?;
    let stage = fields(&staged, "peritus-qualification journal-corruption-stage ")?;
    let request = digest_field(stage.first().copied(), "request_sha256")?;
    let original = digest_field(stage.get(1).copied(), "original_frame_sha256")?;
    let corrupt = digest_field(stage.get(2).copied(), "corrupt_frame_sha256")?;
    if request == original || original == corrupt {
        return Err(io::Error::other("journal corruption identities are not distinct"));
    }
    if field(stage.get(3).copied(), "event_count")? != "1"
        || field(stage.get(4).copied(), "corruption_detected")? != "true"
    {
        return Err(io::Error::other("journal corruption checkpoint differs"));
    }

    let recovered = run(&environment, "qualify-journal-corruption-recover")?;
    let recovery = fields(&recovered, "peritus-qualification journal-corruption-recover ")?;
    if field(recovery.first().copied(), "startup_error_code")? != "PERITUS-DAEMON-STATE-001"
        || digest_field(recovery.get(1).copied(), "corrupt_frame_sha256")? != corrupt
        || field(recovery.get(2).copied(), "event_count")? != "1"
        || field(recovery.get(3).copied(), "aggregate_heads")? != "1"
        || field(recovery.get(4).copied(), "state_records")? != "1"
        || field(recovery.get(5).copied(), "authority_epochs")? != "0"
        || field(recovery.get(6).copied(), "application_principals")? != "0"
        || field(recovery.get(7).copied(), "corruption_detected")? != "true"
        || field(recovery.get(8).copied(), "mutation_admitted")? != "false"
    {
        return Err(io::Error::other("journal fail-closed startup facts differ"));
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
            "journal corruption qualifier exited with {status}: {stderr}"
        )));
    }
    one_line(&stdout)
}

fn read_pipe(pipe: Option<impl Read>) -> io::Result<String> {
    let mut bytes = Vec::new();
    pipe.ok_or_else(|| io::Error::other("journal corruption subprocess pipe is unavailable"))?
        .take((OUTPUT_BOUND + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > OUTPUT_BOUND {
        return Err(io::Error::other("journal corruption output exceeded its bound"));
    }
    String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn one_line(value: &str) -> io::Result<String> {
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.contains(['\r', '\n']) {
        return Err(io::Error::other("journal corruption output is not one line"));
    }
    Ok(value.to_owned())
}

fn fields<'a>(line: &'a str, prefix: &str) -> io::Result<Vec<&'a str>> {
    line.strip_prefix(prefix)
        .map(|value| value.split_ascii_whitespace().collect())
        .ok_or_else(|| io::Error::other("journal corruption observation prefix differs"))
}

fn digest_field<'a>(value: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let value = field(value, name)?;
    if value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(value)
    } else {
        Err(io::Error::other("journal corruption digest is not canonical SHA-256"))
    }
}

fn field<'a>(value: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let (observed, value) = value
        .and_then(|value| value.split_once('='))
        .ok_or_else(|| io::Error::other("journal corruption observation field is missing"))?;
    if observed != name || value.is_empty() {
        return Err(io::Error::other("journal corruption observation field differs"));
    }
    Ok(value)
}

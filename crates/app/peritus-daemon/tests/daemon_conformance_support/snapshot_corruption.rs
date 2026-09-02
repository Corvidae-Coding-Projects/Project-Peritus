//! Public-binary retained snapshot corruption and quarantine qualification.

use std::io::{self, Read};
use std::process::{Command, Stdio};

use super::process::{TestEnvironment, peritusd_executable};

const OUTPUT_BOUND: usize = 4_096;

pub fn corruption_recovery() -> io::Result<()> {
    let environment = TestEnvironment::new()?;
    let staged = run(&environment, "qualify-snapshot-corruption-stage")?;
    let stage = fields(&staged, "peritus-qualification snapshot-corruption-stage ", 5)?;
    let expected = object_field(stage[0], "expected_commit")?;
    let divergent = object_field(stage[1], "divergent_commit")?;
    let reference = field(stage[2], "reference")?;
    digest_field(stage[3], "manifest_sha256")?;
    if expected == divergent
        || !reference.starts_with("refs/peritus/workspaces/")
        || field(stage[4], "corruption_detected")? != "true"
    {
        return Err(io::Error::other("snapshot corruption checkpoint differs"));
    }

    let recovered = run(&environment, "qualify-snapshot-corruption-recover")?;
    let recovery = fields(&recovered, "peritus-qualification snapshot-corruption-recover ", 6)?;
    if field(recovery[0], "reference")? != reference
        || !field(recovery[1], "quarantine_reference")?
            .starts_with("refs/peritus/quarantine/workspaces/")
        || object_field(recovery[2], "quarantined_commit")? != divergent
        || field(recovery[3], "journal_verified")? != "true"
        || field(recovery[4], "corruption_detected")? != "true"
        || field(recovery[5], "mutation_admitted")? != "false"
    {
        return Err(io::Error::other("snapshot corruption containment facts differ"));
    }
    Ok(())
}

fn run(environment: &TestEnvironment, command: &str) -> io::Result<String> {
    let mut child = Command::new(peritusd_executable()?)
        .arg(command)
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
            "snapshot corruption qualifier exited with {status}: {stderr}"
        )));
    }
    let line = stdout.trim_end_matches(['\r', '\n']);
    if line.is_empty() || line.contains(['\r', '\n']) {
        return Err(io::Error::other("snapshot corruption output is not one line"));
    }
    Ok(line.to_owned())
}

fn read_pipe(pipe: Option<impl Read>) -> io::Result<String> {
    let mut bytes = Vec::new();
    pipe.ok_or_else(|| io::Error::other("snapshot corruption pipe is unavailable"))?
        .take((OUTPUT_BOUND + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > OUTPUT_BOUND {
        return Err(io::Error::other("snapshot corruption output exceeded its bound"));
    }
    String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn fields<'a>(line: &'a str, prefix: &str, count: usize) -> io::Result<Vec<&'a str>> {
    let fields = line
        .strip_prefix(prefix)
        .ok_or_else(|| io::Error::other("snapshot corruption prefix differs"))?
        .split_ascii_whitespace()
        .collect::<Vec<_>>();
    if fields.len() != count {
        return Err(io::Error::other("snapshot corruption field count differs"));
    }
    Ok(fields)
}

fn object_field<'a>(value: &'a str, name: &str) -> io::Result<&'a str> {
    let value = field(value, name)?;
    if matches!(value.len(), 40 | 64) && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(value)
    } else {
        Err(io::Error::other("snapshot commit is not a canonical object ID"))
    }
}

fn digest_field<'a>(value: &'a str, name: &str) -> io::Result<&'a str> {
    let value = field(value, name)?;
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(value)
    } else {
        Err(io::Error::other("snapshot digest is not canonical SHA-256"))
    }
}

fn field<'a>(value: &'a str, name: &str) -> io::Result<&'a str> {
    let (observed, value) = value
        .split_once('=')
        .ok_or_else(|| io::Error::other("snapshot corruption field is malformed"))?;
    if observed != name || value.is_empty() {
        return Err(io::Error::other("snapshot corruption field differs"));
    }
    Ok(value)
}

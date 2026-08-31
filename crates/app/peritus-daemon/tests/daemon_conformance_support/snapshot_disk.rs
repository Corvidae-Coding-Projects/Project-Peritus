//! Public-binary snapshot manifest quota rejection and fresh-open qualification.

use std::io::{self, Read};
use std::process::{Command, Stdio};

use super::process::{TestEnvironment, peritusd_executable};

const OUTPUT_BOUND: usize = 4_096;

pub fn quota_exhaustion_recovery() -> io::Result<()> {
    let environment = TestEnvironment::new()?;
    let staged = run(&environment, "qualify-disk-snapshot-commit-stage")?;
    let stage = fields(&staged, "peritus-qualification disk-snapshot-commit-stage ", 8)?;
    let filler = digest_field(stage.first().copied(), "filler_sha256")?;
    let tree = object_field(stage.get(1).copied(), "tree")?;
    let reference = reference_field(stage.get(2).copied(), "reference")?;
    let manifest = digest_field(stage.get(3).copied(), "manifest_sha256")?;
    if field(stage.get(4).copied(), "quota_bytes")? != "4096"
        || field(stage.get(5).copied(), "snapshot_refs")? != "0"
        || field(stage.get(6).copied(), "temporary_files")? != "0"
        || field(stage.get(7).copied(), "object_files")? != "1"
    {
        return Err(io::Error::other("snapshot quota checkpoint retained partial state"));
    }

    let recovered = run(&environment, "qualify-disk-snapshot-commit-recover")?;
    let recovery = fields(&recovered, "peritus-qualification disk-snapshot-commit-recover ", 10)?;
    if digest_field(recovery.first().copied(), "filler_sha256")? != filler
        || object_field(recovery.get(1).copied(), "tree")? != tree
        || reference_field(recovery.get(2).copied(), "reference")? != reference
        || digest_field(recovery.get(3).copied(), "manifest_sha256")? != manifest
        || field(recovery.get(4).copied(), "quota_bytes")? != "4096"
        || field(recovery.get(5).copied(), "used_bytes")? != "4096"
        || field(recovery.get(6).copied(), "journal_verified")? != "true"
        || field(recovery.get(7).copied(), "snapshot_refs")? != "0"
        || field(recovery.get(8).copied(), "temporary_files")? != "0"
        || field(recovery.get(9).copied(), "object_files")? != "1"
    {
        return Err(io::Error::other(format!(
            "fresh snapshot quota facts differ: staged={staged}; recovered={recovered}"
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
            "snapshot quota qualifier exited with {status}: {stderr}"
        )));
    }
    one_line(&stdout)
}

fn read_pipe(pipe: Option<impl Read>) -> io::Result<String> {
    let mut bytes = Vec::new();
    pipe.ok_or_else(|| io::Error::other("snapshot quota subprocess pipe is unavailable"))?
        .take((OUTPUT_BOUND + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > OUTPUT_BOUND {
        return Err(io::Error::other("snapshot quota output exceeded its bound"));
    }
    String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn one_line(value: &str) -> io::Result<String> {
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.contains(['\r', '\n']) {
        return Err(io::Error::other("snapshot quota output is not one line"));
    }
    Ok(value.to_owned())
}

fn fields<'a>(line: &'a str, prefix: &str, count: usize) -> io::Result<Vec<&'a str>> {
    let values = line
        .strip_prefix(prefix)
        .ok_or_else(|| io::Error::other("snapshot quota observation prefix differs"))?
        .split_ascii_whitespace()
        .collect::<Vec<_>>();
    if values.len() != count {
        return Err(io::Error::other("snapshot quota observation field count differs"));
    }
    Ok(values)
}

fn object_field<'a>(value: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let value = field(value, name)?;
    if matches!(value.len(), 40 | 64)
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(value)
    } else {
        Err(io::Error::other("snapshot quota tree is not a canonical object ID"))
    }
}

fn digest_field<'a>(value: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let value = field(value, name)?;
    if value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(value)
    } else {
        Err(io::Error::other("snapshot quota digest is not canonical SHA-256"))
    }
}

fn reference_field<'a>(value: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let value = field(value, name)?;
    if value.starts_with("refs/peritus/workspaces/") && value.len() <= 256 {
        Ok(value)
    } else {
        Err(io::Error::other("snapshot quota reference is not canonical"))
    }
}

fn field<'a>(value: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let (observed, value) = value
        .and_then(|value| value.split_once('='))
        .ok_or_else(|| io::Error::other("snapshot quota observation field is missing"))?;
    if observed != name || value.is_empty() {
        return Err(io::Error::other("snapshot quota observation field differs"));
    }
    Ok(value)
}

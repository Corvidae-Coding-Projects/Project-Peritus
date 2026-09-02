//! Public-binary provider, product-tool, and worker failure qualification.

use std::io::{self, Read as _};
use std::process::{Command, Stdio};

use super::process::{TestEnvironment, peritusd_executable};

const OUTPUT_BOUND: usize = 8_192;

pub fn dependency_failure_recovery() -> io::Result<()> {
    for dependency in ["provider", "tool", "worker"] {
        for fault in ["death", "retry-exhaustion"] {
            run_case(dependency, fault)?;
        }
    }
    Ok(())
}

fn run_case(dependency: &str, fault: &str) -> io::Result<()> {
    let environment = TestEnvironment::new()?;
    let retry_limit = "3";
    let stage = run(&environment, "qualify-dependency-stage", dependency, fault, retry_limit)?;
    let staged = fields(&stage, "peritus-qualification dependency-stage ", 8)?;
    exact(staged[0], "dependency", dependency)?;
    exact(staged[1], "fault", fault)?;
    let staged_state = digest(staged[2], "state_sha256")?;
    digest(staged[3], "effect_sha256")?;
    let expected_attempts = if fault == "death" { "1" } else { retry_limit };
    let expected_stage_events = if fault == "death" { "6" } else { "14" };
    exact(staged[4], "attempts", expected_attempts)?;
    exact(staged[5], "committed_events", expected_stage_events)?;
    let receipt_bytes = number(staged[6], "receipt_bytes")?;
    if dependency == "tool" && receipt_bytes == 0 || dependency != "tool" && receipt_bytes != 0 {
        return Err(io::Error::other("dependency receipt accounting differs"));
    }
    let expected_exit = if dependency == "worker" { "none" } else { "17" };
    exact(staged[7], "child_exit", expected_exit)?;

    let recovery = run(&environment, "qualify-dependency-recover", dependency, fault, retry_limit)?;
    let recovered = fields(&recovery, "peritus-qualification dependency-recover ", 10)?;
    exact(recovered[0], "dependency", dependency)?;
    exact(recovered[1], "fault", fault)?;
    let recovered_state = digest(recovered[2], "state_sha256")?;
    exact(recovered[3], "attempts", expected_attempts)?;
    let expected_recovery_events = if fault == "death" { "7" } else { "14" };
    exact(recovered[4], "committed_events", expected_recovery_events)?;
    exact(recovered[5], "aggregate_heads", "1")?;
    exact(recovered[6], "retry_pending", "false")?;
    exact(recovered[7], "exhausted", if fault == "death" { "false" } else { "true" })?;
    exact(recovered[8], "ownership_reconciled", "true")?;
    exact(recovered[9], "journal_verified", "true")?;
    if (fault == "death") == (staged_state == recovered_state) {
        return Err(io::Error::other("dependency recovery state transition differs"));
    }
    Ok(())
}

fn run(
    environment: &TestEnvironment,
    command: &str,
    dependency: &str,
    fault: &str,
    retry_limit: &str,
) -> io::Result<String> {
    let mut child = Command::new(peritusd_executable()?)
        .arg(command)
        .arg(dependency)
        .arg(fault)
        .arg("--attempts")
        .arg(retry_limit)
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
            "dependency qualifier exited with {status}: {stderr}"
        )));
    }
    let line = stdout.trim_end_matches(['\r', '\n']);
    if line.is_empty() || line.contains(['\r', '\n']) {
        return Err(io::Error::other("dependency output is not one line"));
    }
    Ok(line.to_owned())
}

fn read_pipe(pipe: Option<impl io::Read>) -> io::Result<String> {
    let mut bytes = Vec::new();
    pipe.ok_or_else(|| io::Error::other("dependency pipe is unavailable"))?
        .take((OUTPUT_BOUND + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > OUTPUT_BOUND {
        return Err(io::Error::other("dependency output exceeded its bound"));
    }
    String::from_utf8(bytes).map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn fields<'a>(line: &'a str, prefix: &str, count: usize) -> io::Result<Vec<&'a str>> {
    let values = line
        .strip_prefix(prefix)
        .ok_or_else(|| io::Error::other("dependency prefix differs"))?
        .split_ascii_whitespace()
        .collect::<Vec<_>>();
    if values.len() != count {
        return Err(io::Error::other("dependency field count differs"));
    }
    Ok(values)
}

fn exact(value: &str, name: &str, expected: &str) -> io::Result<()> {
    if field(value, name)? == expected {
        Ok(())
    } else {
        Err(io::Error::other("dependency field differs"))
    }
}

fn digest<'a>(value: &'a str, name: &str) -> io::Result<&'a str> {
    let value = field(value, name)?;
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(value)
    } else {
        Err(io::Error::other("dependency digest is not canonical SHA-256"))
    }
}

fn number(value: &str, name: &str) -> io::Result<u64> {
    field(value, name)?
        .parse()
        .map_err(|_| io::Error::other("dependency count is not an unsigned integer"))
}

fn field<'a>(value: &'a str, name: &str) -> io::Result<&'a str> {
    let (observed, value) =
        value.split_once('=').ok_or_else(|| io::Error::other("dependency field is malformed"))?;
    if observed == name && !value.is_empty() {
        Ok(value)
    } else {
        Err(io::Error::other("dependency field name differs"))
    }
}

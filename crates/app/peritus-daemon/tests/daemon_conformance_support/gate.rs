//! Public-binary gate commit crash qualification.

use std::io::{self, BufRead as _, BufReader, Read};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use super::process::{TestEnvironment, peritusd_executable};

const PROCESS_BOUND: Duration = Duration::from_secs(10);
const OUTPUT_BOUND: usize = 4_096;

pub fn commit_crash_recovery() -> io::Result<()> {
    qualify(false)?;
    qualify(true)
}

fn qualify(after_commit: bool) -> io::Result<()> {
    let environment = TestEnvironment::new()?;
    let timing = if after_commit { "after" } else { "before" };
    let staged = stage_and_kill(&environment, &format!("qualify-gate-{timing}-stage"))?;
    let stage = fields(&staged, &format!("peritus-qualification gate-{timing}-stage "))?;
    let request = field(stage.first().copied(), "request_sha256")?;
    let plan = field(stage.get(1).copied(), "plan_sha256")?;
    let expected_successor = field(stage.get(2).copied(), "successor_sha256")?;
    require_sha256(request)?;
    require_sha256(plan)?;
    require_sha256(expected_successor)?;
    let (successor, checkpoint, revision, position) = if after_commit {
        if field(stage.get(6).copied(), "committed")? != "true" {
            return Err(io::Error::other("gate checkpoint was not committed"));
        }
        (
            expected_successor,
            field(stage.get(3).copied(), "checkpoint_sha256")?,
            field(stage.get(4).copied(), "state_revision")?,
            field(stage.get(5).copied(), "producing_position")?,
        )
    } else {
        ("none", "none", "none", "none")
    };
    if after_commit {
        require_sha256(checkpoint)?;
    }
    let recovered = run_to_exit(&environment, &format!("qualify-gate-{timing}-recover"))?;
    let recovery = fields(&recovered, &format!("peritus-qualification gate-{timing}-recover "))?;
    let count = u8::from(after_commit).to_string();
    if field(recovery.first().copied(), "request_sha256")? != request
        || field(recovery.get(1).copied(), "plan_sha256")? != plan
        || field(recovery.get(2).copied(), "journal_verified")? != "true"
        || field(recovery.get(3).copied(), "committed_events")? != count
        || field(recovery.get(4).copied(), "aggregate_heads")? != count
        || field(recovery.get(5).copied(), "state_revision")? != revision
        || field(recovery.get(6).copied(), "successor_sha256")? != successor
        || field(recovery.get(7).copied(), "checkpoint_sha256")? != checkpoint
        || field(recovery.get(8).copied(), "producing_position")? != position
    {
        return Err(io::Error::other("gate recovery facts differ from the commit boundary"));
    }
    Ok(())
}

fn stage_and_kill(environment: &TestEnvironment, command_name: &str) -> io::Result<String> {
    let mut child = command(environment, command_name)?.spawn()?;
    let stdout = child.stdout.take().ok_or_else(|| io::Error::other("gate stage has no stdout"))?;
    let line = match read_line(stdout) {
        Ok(line) => line,
        Err(error) => {
            let _ = child.wait();
            return Err(failure(&mut child, &format!("gate checkpoint read failed: {error}")));
        }
    };
    if child.try_wait()?.is_some() {
        return Err(failure(&mut child, "gate stage exited before its checkpoint"));
    }
    child.kill()?;
    child.wait()?;
    let stderr = read_pipe(child.stderr.take())?;
    if !stderr.is_empty() {
        return Err(io::Error::other(format!("gate stage wrote diagnostics: {stderr}")));
    }
    Ok(line)
}

fn run_to_exit(environment: &TestEnvironment, command_name: &str) -> io::Result<String> {
    let mut child = command(environment, command_name)?.spawn()?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if started.elapsed() >= PROCESS_BOUND {
            child.kill()?;
            child.wait()?;
            return Err(io::Error::new(io::ErrorKind::TimedOut, "gate recovery timed out"));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = read_pipe(child.stdout.take())?;
    let stderr = read_pipe(child.stderr.take())?;
    if !status.success() || !stderr.is_empty() {
        return Err(io::Error::other(format!("gate recovery exited with {status}: {stderr}")));
    }
    one_line(&stdout)
}

fn command(environment: &TestEnvironment, name: &str) -> io::Result<Command> {
    let mut command = Command::new(peritusd_executable()?);
    command
        .arg(name)
        .arg("--config")
        .arg(environment.config_path())
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    Ok(command)
}

fn read_line(reader: impl Read) -> io::Result<String> {
    let mut bytes = Vec::new();
    BufReader::new(reader).take((OUTPUT_BOUND + 1) as u64).read_until(b'\n', &mut bytes)?;
    if bytes.is_empty() || bytes.len() > OUTPUT_BOUND || bytes.last() != Some(&b'\n') {
        return Err(io::Error::other("gate checkpoint is empty, oversized, or unterminated"));
    }
    bytes.pop();
    String::from_utf8(bytes).map_err(invalid)
}

fn read_pipe(pipe: Option<impl Read>) -> io::Result<String> {
    let mut bytes = Vec::new();
    pipe.ok_or_else(|| io::Error::other("gate subprocess pipe is unavailable"))?
        .take((OUTPUT_BOUND + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > OUTPUT_BOUND {
        return Err(io::Error::other("gate subprocess output exceeded its bound"));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn failure(child: &mut Child, message: &str) -> io::Error {
    let stderr = read_pipe(child.stderr.take()).unwrap_or_default();
    io::Error::other(format!("{message}: {stderr}"))
}

fn one_line(value: &str) -> io::Result<String> {
    let value = value.trim_end_matches(['\r', '\n']);
    if value.is_empty() || value.contains(['\r', '\n']) {
        return Err(io::Error::other("gate recovery output is not one line"));
    }
    Ok(value.to_owned())
}

fn fields<'a>(line: &'a str, prefix: &str) -> io::Result<Vec<&'a str>> {
    line.strip_prefix(prefix)
        .map(|value| value.split_ascii_whitespace().collect())
        .ok_or_else(|| io::Error::other("gate observation prefix differs"))
}

fn field<'a>(field: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let (observed, value) = field
        .and_then(|field| field.split_once('='))
        .ok_or_else(|| io::Error::other("gate observation field is missing"))?;
    if observed != name || value.is_empty() {
        return Err(io::Error::other("gate observation field differs"));
    }
    Ok(value)
}

fn require_sha256(value: &str) -> io::Result<()> {
    if value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(io::Error::other("gate digest is not canonical SHA-256"))
    }
}

fn invalid(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

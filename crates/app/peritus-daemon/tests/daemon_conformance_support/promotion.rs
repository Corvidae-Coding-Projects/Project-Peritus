//! Public-binary F0 atomic promotion crash qualification.

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
    let staged = stage_and_kill(&environment, &format!("qualify-promotion-{timing}-stage"))?;
    let stage = fields(&staged, &format!("peritus-qualification promotion-{timing}-stage "))?;
    let proposal = digest_field(stage.first().copied(), "proposal_sha256")?;
    let authorization = digest_field(stage.get(1).copied(), "authorization_sha256")?;
    let campaign_before = digest_field(stage.get(2).copied(), "campaign_before_sha256")?;
    let pointer_before = digest_field(stage.get(3).copied(), "pointer_before_sha256")?;
    let campaign_after = digest_field(stage.get(4).copied(), "campaign_after_sha256")?;
    let pointer_after = digest_field(stage.get(5).copied(), "pointer_after_sha256")?;
    if after_commit
        && (field(stage.get(6).copied(), "approval_revision")? != "2"
            || field(stage.get(7).copied(), "first_position")? != "15"
            || field(stage.get(8).copied(), "last_position")? != "16"
            || field(stage.get(9).copied(), "committed")? != "true")
    {
        return Err(io::Error::other("promotion stage receipt is not complete"));
    }
    let recovered = run_to_exit(&environment, &format!("qualify-promotion-{timing}-recover"))?;
    let recovery =
        fields(&recovered, &format!("peritus-qualification promotion-{timing}-recover "))?;
    let expected_authorization = if after_commit { authorization } else { "none" };
    let expected_campaign = if after_commit { campaign_after } else { campaign_before };
    let expected_pointer = if after_commit { pointer_after } else { pointer_before };
    let expected_revision = if after_commit { "2" } else { "1" };
    let expected_events = if after_commit { "16" } else { "14" };
    let expected_committed = if after_commit { "true" } else { "false" };
    if field(recovery.first().copied(), "proposal_sha256")? != proposal
        || field(recovery.get(1).copied(), "authorization_sha256")? != expected_authorization
        || field(recovery.get(2).copied(), "campaign_sha256")? != expected_campaign
        || field(recovery.get(3).copied(), "pointer_sha256")? != expected_pointer
        || field(recovery.get(4).copied(), "approval_revision")? != expected_revision
        || field(recovery.get(5).copied(), "approval_position")? != expected_events
        || field(recovery.get(6).copied(), "committed_events")? != expected_events
        || field(recovery.get(7).copied(), "aggregate_heads")? != "4"
        || field(recovery.get(8).copied(), "committed")? != expected_committed
    {
        return Err(io::Error::other("promotion recovery facts differ from the commit boundary"));
    }
    Ok(())
}

fn stage_and_kill(environment: &TestEnvironment, command_name: &str) -> io::Result<String> {
    let mut child = command(environment, command_name)?.spawn()?;
    let stdout =
        child.stdout.take().ok_or_else(|| io::Error::other("promotion stage has no stdout"))?;
    let line = match read_line(stdout) {
        Ok(line) => line,
        Err(error) => {
            let _ = child.wait();
            return Err(failure(&mut child, &format!("promotion checkpoint read failed: {error}")));
        }
    };
    if child.try_wait()?.is_some() {
        return Err(failure(&mut child, "promotion stage exited before its checkpoint"));
    }
    child.kill()?;
    child.wait()?;
    let stderr = read_pipe(child.stderr.take())?;
    if !stderr.is_empty() {
        return Err(io::Error::other(format!("promotion stage wrote diagnostics: {stderr}")));
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
            return Err(io::Error::new(io::ErrorKind::TimedOut, "promotion recovery timed out"));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = read_pipe(child.stdout.take())?;
    let stderr = read_pipe(child.stderr.take())?;
    if !status.success() || !stderr.is_empty() {
        return Err(io::Error::other(format!("promotion recovery exited with {status}: {stderr}")));
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
        return Err(io::Error::other("promotion checkpoint is empty, oversized, or unterminated"));
    }
    bytes.pop();
    String::from_utf8(bytes).map_err(invalid)
}

fn read_pipe(pipe: Option<impl Read>) -> io::Result<String> {
    let mut bytes = Vec::new();
    pipe.ok_or_else(|| io::Error::other("promotion subprocess pipe is unavailable"))?
        .take((OUTPUT_BOUND + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > OUTPUT_BOUND {
        return Err(io::Error::other("promotion subprocess output exceeded its bound"));
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
        return Err(io::Error::other("promotion recovery output is not one line"));
    }
    Ok(value.to_owned())
}

fn fields<'a>(line: &'a str, prefix: &str) -> io::Result<Vec<&'a str>> {
    line.strip_prefix(prefix)
        .map(|value| value.split_ascii_whitespace().collect())
        .ok_or_else(|| io::Error::other("promotion observation prefix differs"))
}

fn digest_field<'a>(value: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let value = field(value, name)?;
    if value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(value)
    } else {
        Err(io::Error::other("promotion digest is not canonical SHA-256"))
    }
}

fn field<'a>(value: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let (observed, value) = value
        .and_then(|value| value.split_once('='))
        .ok_or_else(|| io::Error::other("promotion observation field is missing"))?;
    if observed != name || value.is_empty() {
        return Err(io::Error::other("promotion observation field differs"));
    }
    Ok(value)
}

fn invalid(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

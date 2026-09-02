//! Public-binary artifact commit crash qualification.

use std::io::{self, BufRead, BufReader, Read};
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
    let stage = format!("qualify-blob-{timing}-stage");
    let recover = format!("qualify-blob-{timing}-recover");
    let line = stage_and_kill(&environment, &stage)?;
    let checkpoint = parse_fields(&line, &format!("peritus-qualification blob-{timing}-stage "))?;
    let digest = field(checkpoint.first().copied(), "digest")?;
    require_sha256(digest)?;
    let bytes = field(checkpoint.get(1).copied(), "bytes")?.parse::<u64>().map_err(invalid)?;
    let recovered = run_to_exit(&environment, &recover)?;
    let recovery =
        parse_fields(&recovered, &format!("peritus-qualification blob-{timing}-recover "))?;
    if field(recovery.first().copied(), "digest")? != digest
        || field(recovery.get(1).copied(), "bytes")?.parse::<u64>().map_err(invalid)? != bytes
        || field(recovery.get(2).copied(), "journal_verified")? != "true"
        || field(recovery.get(3).copied(), "finalized")? != after_commit.to_string()
        || field(recovery.get(4).copied(), "referenced")? != after_commit.to_string()
        || field(recovery.get(5).copied(), "temporary_files")? != "0"
        || field(recovery.get(6).copied(), "object_files")? != u8::from(after_commit).to_string()
    {
        return Err(io::Error::other("blob recovery facts differ from the commit boundary"));
    }
    Ok(())
}

fn stage_and_kill(environment: &TestEnvironment, command_name: &str) -> io::Result<String> {
    let mut child = command(environment, command_name)?.spawn()?;
    let stdout =
        child.stdout.take().ok_or_else(|| io::Error::other("blob stage has no stdout pipe"))?;
    let line = read_line(stdout)?;
    if child.try_wait()?.is_some() {
        return Err(failure(&mut child, "blob stage exited before its checkpoint"));
    }
    child.kill()?;
    child.wait()?;
    let stderr = read_pipe(child.stderr.take())?;
    if !stderr.is_empty() {
        return Err(io::Error::other(format!("blob stage wrote diagnostics: {stderr}")));
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
            return Err(io::Error::new(io::ErrorKind::TimedOut, "blob recovery timed out"));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = read_pipe(child.stdout.take())?;
    let stderr = read_pipe(child.stderr.take())?;
    if !status.success() || !stderr.is_empty() {
        return Err(io::Error::other(format!("blob recovery exited with {status}: {stderr}")));
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
        return Err(io::Error::other("blob checkpoint is empty, oversized, or unterminated"));
    }
    bytes.pop();
    String::from_utf8(bytes).map_err(invalid)
}

fn read_pipe(pipe: Option<impl Read>) -> io::Result<String> {
    let pipe = pipe.ok_or_else(|| io::Error::other("blob subprocess pipe is unavailable"))?;
    let mut bytes = Vec::new();
    pipe.take((OUTPUT_BOUND + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > OUTPUT_BOUND {
        return Err(io::Error::other("blob subprocess output exceeded its bound"));
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
        return Err(io::Error::other("blob recovery output is not one line"));
    }
    Ok(value.to_owned())
}

fn parse_fields<'a>(line: &'a str, prefix: &str) -> io::Result<Vec<&'a str>> {
    line.strip_prefix(prefix)
        .map(|value| value.split_ascii_whitespace().collect())
        .ok_or_else(|| io::Error::other("blob observation prefix differs"))
}

fn field<'a>(field: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let (observed, value) = field
        .and_then(|field| field.split_once('='))
        .ok_or_else(|| io::Error::other("blob observation field is missing"))?;
    if observed != name || value.is_empty() {
        return Err(io::Error::other("blob observation field differs"));
    }
    Ok(value)
}

fn require_sha256(value: &str) -> io::Result<()> {
    if value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(io::Error::other("blob digest is not canonical SHA-256"))
    }
}

fn invalid(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

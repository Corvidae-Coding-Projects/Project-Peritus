//! Public-binary retained Git snapshot commit crash qualification.

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
    let staged = stage_and_kill(&environment, &format!("qualify-snapshot-{timing}-stage"))?;
    let stage = fields(&staged, &format!("peritus-qualification snapshot-{timing}-stage "))?;
    let (commit, tree, reference, manifest) = if after_commit {
        (
            field(stage.first().copied(), "commit")?,
            field(stage.get(1).copied(), "tree")?,
            field(stage.get(2).copied(), "reference")?,
            field(stage.get(3).copied(), "manifest_sha256")?,
        )
    } else {
        (
            "none",
            field(stage.first().copied(), "tree")?,
            field(stage.get(1).copied(), "reference")?,
            "none",
        )
    };
    require_object_id(tree)?;
    if after_commit {
        require_object_id(commit)?;
        require_sha256(manifest)?;
        if field(stage.get(4).copied(), "retained")? != "true" {
            return Err(io::Error::other(
                "snapshot was not retained at its post-commit checkpoint",
            ));
        }
    }
    let recovered = run_to_exit(&environment, &format!("qualify-snapshot-{timing}-recover"))?;
    let recovery =
        fields(&recovered, &format!("peritus-qualification snapshot-{timing}-recover "))?;
    if field(recovery.first().copied(), "commit")? != commit
        || field(recovery.get(1).copied(), "tree")? != tree
        || field(recovery.get(2).copied(), "reference")? != reference
        || field(recovery.get(3).copied(), "manifest_sha256")? != manifest
        || field(recovery.get(4).copied(), "journal_verified")? != "true"
        || field(recovery.get(5).copied(), "retained")? != after_commit.to_string()
        || field(recovery.get(6).copied(), "snapshot_refs")? != u8::from(after_commit).to_string()
    {
        return Err(io::Error::other("snapshot recovery facts differ from the commit boundary"));
    }
    Ok(())
}

fn stage_and_kill(environment: &TestEnvironment, command_name: &str) -> io::Result<String> {
    let mut child = command(environment, command_name)?.spawn()?;
    let stdout =
        child.stdout.take().ok_or_else(|| io::Error::other("snapshot stage has no stdout"))?;
    let line = match read_line(stdout) {
        Ok(line) => line,
        Err(error) => {
            let _ = child.wait();
            return Err(failure(&mut child, &format!("snapshot checkpoint read failed: {error}")));
        }
    };
    if child.try_wait()?.is_some() {
        return Err(failure(&mut child, "snapshot stage exited before its checkpoint"));
    }
    child.kill()?;
    child.wait()?;
    let stderr = read_pipe(child.stderr.take())?;
    if !stderr.is_empty() {
        return Err(io::Error::other(format!("snapshot stage wrote diagnostics: {stderr}")));
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
            return Err(io::Error::new(io::ErrorKind::TimedOut, "snapshot recovery timed out"));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = read_pipe(child.stdout.take())?;
    let stderr = read_pipe(child.stderr.take())?;
    if !status.success() || !stderr.is_empty() {
        return Err(io::Error::other(format!("snapshot recovery exited with {status}: {stderr}")));
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
        return Err(io::Error::other("snapshot checkpoint is empty, oversized, or unterminated"));
    }
    bytes.pop();
    String::from_utf8(bytes).map_err(invalid)
}

fn read_pipe(pipe: Option<impl Read>) -> io::Result<String> {
    let mut bytes = Vec::new();
    pipe.ok_or_else(|| io::Error::other("snapshot subprocess pipe is unavailable"))?
        .take((OUTPUT_BOUND + 1) as u64)
        .read_to_end(&mut bytes)?;
    if bytes.len() > OUTPUT_BOUND {
        return Err(io::Error::other("snapshot subprocess output exceeded its bound"));
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
        return Err(io::Error::other("snapshot recovery output is not one line"));
    }
    Ok(value.to_owned())
}

fn fields<'a>(line: &'a str, prefix: &str) -> io::Result<Vec<&'a str>> {
    line.strip_prefix(prefix)
        .map(|value| value.split_ascii_whitespace().collect())
        .ok_or_else(|| io::Error::other("snapshot observation prefix differs"))
}

fn field<'a>(field: Option<&'a str>, name: &str) -> io::Result<&'a str> {
    let (observed, value) = field
        .and_then(|field| field.split_once('='))
        .ok_or_else(|| io::Error::other("snapshot observation field is missing"))?;
    if observed != name || value.is_empty() {
        return Err(io::Error::other("snapshot observation field differs"));
    }
    Ok(value)
}

fn require_object_id(value: &str) -> io::Result<()> {
    if matches!(value.len(), 40 | 64) && lower_hex(value) {
        Ok(())
    } else {
        Err(io::Error::other("snapshot object ID is not canonical"))
    }
}

fn require_sha256(value: &str) -> io::Result<()> {
    if value.len() == 64 && lower_hex(value) {
        Ok(())
    } else {
        Err(io::Error::other("snapshot manifest digest is not canonical"))
    }
}

fn lower_hex(value: &str) -> bool {
    value.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn invalid(error: impl std::error::Error + Send + Sync + 'static) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, error)
}

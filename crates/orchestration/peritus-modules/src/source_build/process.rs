//! Bounded systemd-owned process scopes used by source preparation and offline compilation.

use crate::{Error, Registry, invalid};
use std::{
    fs,
    io::Read as _,
    os::unix::process::CommandExt as _,
    path::Path,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

const OUTPUT_BYTES: u64 = 16 * 1024 * 1024;
const STORAGE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const WALL: Duration = Duration::from_hours(1);

pub(super) struct ScopedRun<'a> {
    pub stage: &'a str,
    pub program: &'a Path,
    pub arguments: &'a [String],
    pub environment: &'a [(String, String)],
    pub scratch: &'a Path,
}

pub(super) fn run_scoped(
    registry: &Registry,
    module_id: &str,
    job_id: &str,
    run: &ScopedRun<'_>,
    cancelled: &Arc<AtomicBool>,
) -> Result<(), Error> {
    let ScopedRun { stage, program, arguments, environment, scratch } = *run;
    let short = job_id.get(..12).ok_or_else(|| invalid("invalid source-build job identity"))?;
    if !stage.bytes().all(|byte| byte.is_ascii_lowercase() || byte == b'-') {
        return Err(invalid("invalid source-build stage identity"));
    }
    let unit_base = format!("peritus-module-build-{short}-{stage}");
    let unit = format!("{unit_base}.scope");
    let stdout_path = scratch.join(format!("{stage}.stdout"));
    let stderr_path = scratch.join(format!("{stage}.stderr"));
    let stdout = fs::OpenOptions::new().write(true).create_new(true).open(&stdout_path)?;
    let stderr = fs::OpenOptions::new().write(true).create_new(true).open(&stderr_path)?;
    let mut command = Command::new("/usr/bin/systemd-run");
    command
        .env_clear()
        .args([
            "--user",
            "--scope",
            "--collect",
            "--quiet",
            &format!("--unit={unit_base}"),
            "--property=MemoryMax=6442450944",
            "--property=MemorySwapMax=1073741824",
            "--property=TasksMax=192",
            "--property=CPUQuota=200%",
            "--property=RuntimeMaxSec=3600s",
            "--",
        ])
        .arg(program)
        .args(arguments)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::from(stderr));
    for name in ["XDG_RUNTIME_DIR", "DBUS_SESSION_BUS_ADDRESS"] {
        if let Some(value) = std::env::var_os(name) {
            command.env(name, value);
        }
    }
    for (name, value) in environment {
        command.env(name, value);
    }
    command.process_group(0);
    super::set_unit(registry, module_id, Some(unit.clone()))?;
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            super::set_unit(registry, module_id, None)?;
            return Err(invalid(format!(
                "bounded systemd user scope could not start: {error}; a working user session bus is required"
            )));
        }
    };
    let started = Instant::now();
    let mut last_storage_check = Instant::now();
    let outcome = loop {
        if cancelled.load(Ordering::Acquire) {
            terminate_unit(&unit);
            let _ = child.kill();
            break Err(invalid(
                "source build was cancelled; its bounded process scope was stopped",
            ));
        }
        if started.elapsed() > WALL {
            terminate_unit(&unit);
            let _ = child.kill();
            break Err(invalid("source build exceeded the one-hour native wall limit"));
        }
        let output = size(&stdout_path).saturating_add(size(&stderr_path));
        if output > OUTPUT_BYTES {
            terminate_unit(&unit);
            let _ = child.kill();
            break Err(invalid("source build exceeded the 16 MiB retained-output limit"));
        }
        if last_storage_check.elapsed() >= Duration::from_secs(1) {
            if directory_size(scratch, STORAGE_BYTES)? > STORAGE_BYTES {
                terminate_unit(&unit);
                let _ = child.kill();
                break Err(invalid("source build exceeded the 8 GiB private-storage limit"));
            }
            last_storage_check = Instant::now();
        }
        match child.try_wait()? {
            Some(status) if status.success() => break Ok(()),
            Some(status) => {
                break Err(invalid(format!(
                    "{stage} exited with {status}; {}",
                    tail(&stderr_path, 4096)
                        .unwrap_or_else(|_| "diagnostic output unavailable".into())
                )));
            }
            None => thread::sleep(Duration::from_millis(100)),
        }
    };
    let _ = child.wait();
    super::set_unit(registry, module_id, None)?;
    append_log(registry, module_id, stage, &stdout_path, &stderr_path)?;
    outcome
}

pub(super) fn terminate_unit(unit: &str) {
    if !valid_unit(unit) {
        return;
    }
    let _ = Command::new("/usr/bin/systemctl")
        .args(["--user", "kill", "--kill-whom=all", unit])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[expect(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the exact lowercase systemd unit suffix is part of the security boundary"
)]
fn valid_unit(value: &str) -> bool {
    value.starts_with("peritus-module-build-")
        && value.ends_with(".scope")
        && value.len() <= 96
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        })
}

fn append_log(
    registry: &Registry,
    module_id: &str,
    stage: &str,
    stdout: &Path,
    stderr: &Path,
) -> Result<(), Error> {
    use std::io::Write as _;
    let path = super::build_log(registry, module_id);
    let mut log = fs::OpenOptions::new().create(true).append(true).open(path)?;
    writeln!(log, "\n== {stage} stdout ==")?;
    copy_bounded(stdout, &mut log)?;
    writeln!(log, "\n== {stage} stderr ==")?;
    copy_bounded(stderr, &mut log)?;
    log.sync_all()?;
    Ok(())
}

fn copy_bounded(path: &Path, output: &mut fs::File) -> Result<(), Error> {
    let mut input = fs::File::open(path)?.take(OUTPUT_BYTES);
    std::io::copy(&mut input, output)?;
    Ok(())
}

fn tail(path: &Path, maximum: usize) -> Result<String, Error> {
    let bytes = fs::read(path)?;
    let start = bytes.len().saturating_sub(maximum);
    Ok(String::from_utf8_lossy(&bytes[start..]).trim().to_owned())
}

fn size(path: &Path) -> u64 {
    fs::metadata(path).map_or(0, |metadata| metadata.len())
}

fn directory_size(path: &Path, stop_after: u64) -> Result<u64, Error> {
    let mut total = 0u64;
    let mut pending = vec![path.to_path_buf()];
    while let Some(directory) = pending.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                pending.push(entry.path());
            } else if metadata.is_file() {
                total = total.saturating_add(metadata.len());
                if total > stop_after {
                    return Ok(total);
                }
            }
        }
    }
    Ok(total)
}

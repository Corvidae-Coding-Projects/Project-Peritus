//! Canonical package handoff evidence and durable publication.

use std::{fs::{self, OpenOptions}, io::Write as _, path::{Path, PathBuf}};

#[cfg(unix)]
use std::fs::File;

use super::{corrupt, storage};
use crate::{DaemonError, DaemonIdentity, instance::record::InstanceRecord};

pub(super) fn completion_status<'a>(
    bytes: &'a [u8],
    identity: &DaemonIdentity,
) -> Result<&'a str, DaemonError> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| corrupt("retained package handoff outcome is not UTF-8"))?;
    let mut lines = text.lines();
    if lines.next() != Some("peritus-package-handoff-v1") {
        return Err(corrupt("retained package handoff outcome has an invalid version"));
    }
    let endpoint = completion_field(lines.next(), "endpoint=")?;
    let pid = completion_field(lines.next(), "pid=")?
        .parse::<u32>()
        .map_err(|_| corrupt("retained package handoff PID is malformed"))?;
    let start_token = completion_field(lines.next(), "start_token=")?
        .parse::<u64>()
        .map_err(|_| corrupt("retained package handoff birth token is malformed"))?;
    let status = completion_field(lines.next(), "status=")?;
    if lines.next().is_some()
        || endpoint != identity.endpoint_name()
        || pid == 0
        || !matches!(status, "clean" | "unclean" | "failed")
    {
        return Err(corrupt("retained package handoff outcome is inconsistent"));
    }
    let canonical = format!(
        "peritus-package-handoff-v1\nendpoint={endpoint}\npid={pid}\nstart_token={start_token}\nstatus={status}\n"
    );
    if bytes != canonical.as_bytes() {
        return Err(corrupt("retained package handoff outcome is not canonical"));
    }
    Ok(status)
}

fn completion_field<'a>(line: Option<&'a str>, prefix: &str) -> Result<&'a str, DaemonError> {
    line.and_then(|line| line.strip_prefix(prefix))
        .filter(|value| !value.is_empty())
        .ok_or_else(|| corrupt("retained package handoff outcome is incomplete"))
}

pub(super) fn handoff_path(state_root: &Path, record: &InstanceRecord, suffix: &str) -> PathBuf {
    state_root.join(format!(
        ".daemon.handoff.{}.{}.{}",
        record.pid(),
        record.start_token(),
        suffix,
    ))
}

pub(super) fn completion_bytes(record: &InstanceRecord, disposition: &str) -> Vec<u8> {
    format!(
        "peritus-package-handoff-v1\nendpoint={}\npid={}\nstart_token={}\nstatus={}\n",
        record.endpoint(),
        record.pid(),
        record.start_token(),
        disposition,
    )
    .into_bytes()
}

pub(super) fn latest_path(state_root: &Path) -> PathBuf {
    state_root.join(".daemon.handoff.latest")
}

pub(super) fn publish_latest(root: &Path, bytes: &[u8]) -> Result<(), DaemonError> {
    let path = latest_path(root);
    let temporary = root.join(format!(".daemon.handoff.latest.{}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|error| storage("open retained package handoff outcome", error))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|error| storage("write retained package handoff outcome", error))?;
    drop(file);
    fs::rename(&temporary, path)
        .and_then(|()| sync_directory(root))
        .map_err(|error| storage("publish retained package handoff outcome", error))
}

pub(super) fn publish(root: &Path, destination: &Path, bytes: &[u8]) -> Result<(), DaemonError> {
    match fs::read(destination) {
        Ok(existing) if existing == bytes => return Ok(()),
        Ok(_) => return Err(corrupt("package handoff record conflicts with retained evidence")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(storage("inspect package handoff record", error)),
    }
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| corrupt("package handoff record name is not canonical UTF-8"))?;
    let temporary = root.join(format!(".{name}.tmp"));
    let result = match fs::read(&temporary) {
        Ok(existing) if existing == bytes => Ok(()),
        Ok(_) => return Err(corrupt("temporary package handoff evidence conflicts")),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                options.mode(0o600);
            }
            let mut file = options
                .open(&temporary)
                .map_err(|error| storage("create package handoff record", error))?;
            file.write_all(bytes).and_then(|()| file.sync_all())
        }
        Err(error) => return Err(storage("inspect temporary package handoff record", error)),
    }
    .and_then(|()| fs::rename(&temporary, destination))
    .or_else(|error| match fs::read(destination) {
        Ok(existing) if existing == bytes => {
            let _ = fs::remove_file(&temporary);
            Ok(())
        }
        _ => Err(error),
    })
    .and_then(|()| sync_directory(root));
    if let Err(error) = result {
        return Err(storage("publish package handoff record", error));
    }
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> std::io::Result<()> {
    File::open(path)?.sync_all()
}

#[cfg(windows)]
const fn sync_directory(_path: &Path) -> std::io::Result<()> {
    Ok(())
}

pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::with_capacity(bytes.len() * 2), |mut output, byte| {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
        output
    })
}

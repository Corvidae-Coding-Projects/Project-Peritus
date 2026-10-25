//! Cross-process ownership, cancellation and exact Linux process-birth checks.

use crate::{Error, digest, invalid};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{ErrorKind, Write as _},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProcessOwner {
    pid: u32,
    start_token: u64,
}

pub(super) fn cancel_path(root: &Path, id: &str) -> PathBuf {
    build_root(root, id).join("cancel.requested")
}

pub(super) fn active_key(root: &Path, id: &str) -> String {
    digest(&[root.as_os_str().as_encoded_bytes(), b"\0", id.as_bytes()].concat())
}

pub(super) fn current_owner() -> Result<ProcessOwner, Error> {
    let pid = std::process::id();
    let start_token = process_start_token(pid)?
        .ok_or_else(|| invalid("current source-build owner process is not observable"))?;
    Ok(ProcessOwner { pid, start_token })
}

pub(super) fn owner_is_live(owner: &ProcessOwner) -> Result<bool, Error> {
    Ok(process_start_token(owner.pid)?.is_some_and(|token| token == owner.start_token))
}

pub(super) fn owner_holds_lease(root: &Path, id: &str, owner: &ProcessOwner) -> bool {
    fs::read(owner_path(root, id))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<ProcessOwner>(&bytes).ok())
        .as_ref()
        == Some(owner)
}

pub(super) fn claim_owner(root: &Path, id: &str, owner: &ProcessOwner) -> Result<bool, Error> {
    let path = owner_path(root, id);
    let bytes = serde_json::to_vec(owner)?;
    for _ in 0..3 {
        match fs::OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                file.write_all(&bytes)?;
                file.sync_all()?;
                return Ok(true);
            }
            Err(error) if error.kind() == ErrorKind::AlreadyExists => {
                let existing: ProcessOwner = serde_json::from_slice(&fs::read(&path)?)?;
                if owner_is_live(&existing)? {
                    return Ok(existing == *owner);
                }
                fs::remove_file(&path)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Err(invalid("source-build owner changed repeatedly; retry the explicit action"))
}

pub(super) fn release_owner(root: &Path, id: &str, owner: &ProcessOwner) {
    let path = owner_path(root, id);
    if owner_holds_lease(root, id, owner) {
        let _ = fs::remove_file(path);
    }
}

pub(super) fn remove_cancel_request(root: &Path, id: &str) -> Result<(), Error> {
    match fs::remove_file(cancel_path(root, id)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn build_root(root: &Path, id: &str) -> PathBuf {
    root.join("source-builds").join(id)
}

fn owner_path(root: &Path, id: &str) -> PathBuf {
    build_root(root, id).join("owner.json")
}

fn process_start_token(pid: u32) -> Result<Option<u64>, Error> {
    let text = match fs::read_to_string(format!("/proc/{pid}/stat")) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(invalid(format!(
                "cannot establish exact source-build owner identity: {error}"
            )));
        }
    };
    let close =
        text.rfind(')').ok_or_else(|| invalid("source-build owner process status is malformed"))?;
    let token = text
        .get(close + 2..)
        .and_then(|fields| fields.split_ascii_whitespace().nth(19))
        .and_then(|field| field.parse().ok())
        .ok_or_else(|| invalid("source-build owner process has no exact birth token"))?;
    Ok(Some(token))
}

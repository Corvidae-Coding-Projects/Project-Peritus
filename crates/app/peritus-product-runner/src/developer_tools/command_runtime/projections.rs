//! Bounded durable product projections for reconnect-stable command handles.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write as _,
    path::Path,
};

use super::result;

const FILE_NAME: &str = "command-handles-v1.json";
const MAX_PROJECTIONS: usize = 4_096;
const MAX_PROJECTION_BYTES: u64 = 64 * 1_024 * 1_024;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ProjectionFile {
    version: u16,
    entries: Vec<Projection>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Projection {
    handle: String,
    value: Value,
}

pub(super) fn load(root: &Path) -> Result<BTreeMap<String, Value>, String> {
    let path = root.join(FILE_NAME);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => return Err(format!("read command handle projections: {error}")),
    };
    if bytes.len() as u64 > MAX_PROJECTION_BYTES {
        quarantine(&path);
        return Ok(BTreeMap::new());
    }
    let file: ProjectionFile = match serde_json::from_slice::<ProjectionFile>(&bytes) {
        Ok(file) if file.version == 1 && file.entries.len() <= MAX_PROJECTIONS => file,
        Ok(_) | Err(_) => {
            quarantine(&path);
            return Ok(BTreeMap::new());
        }
    };
    let mut recovered = BTreeMap::new();
    for entry in file.entries {
        if entry.handle.is_empty() || entry.handle.len() > 128 {
            continue;
        }
        let value = if entry.value.get("state").and_then(Value::as_str) == Some("running") {
            let execution_mode = entry.value.get("execution_mode").cloned();
            let execution_resources = entry.value.get("execution_resources").cloned();
            let mut value = result::indeterminate(
                &entry.handle,
                "the daemon restarted while this command was active; its durable process record was reconciled, but live control cannot be resumed",
            );
            if let (Some(mode), Some(object)) = (execution_mode, value.as_object_mut()) {
                object.insert("execution_mode".to_owned(), mode);
            }
            if let (Some(resources), Some(object)) = (execution_resources, value.as_object_mut()) {
                object.insert("execution_resources".to_owned(), resources);
            }
            value
        } else {
            entry.value
        };
        recovered.insert(entry.handle, value);
    }
    Ok(recovered)
}

pub(super) fn record(root: &Path, handle: &str, value: Value) -> Result<(), String> {
    let path = root.join(FILE_NAME);
    let mut file = read_file(&path)?;
    // A successful older observation may arrive here after a newer terminal observer released
    // its invocation lock and published. Preserve the durable lifecycle frontier regardless of
    // response/render scheduling; a late active update must not erase known terminal truth.
    if matches!(value.get("state").and_then(Value::as_str), Some("running" | "starting"))
        && file.entries.iter().any(|entry| {
            entry.handle == handle
                && matches!(
                    entry.value.get("state").and_then(Value::as_str),
                    Some("completed" | "indeterminate")
                )
        })
    {
        return Ok(());
    }
    file.entries.retain(|entry| entry.handle != handle);
    file.entries.push(Projection { handle: handle.to_owned(), value });
    if file.entries.len() > MAX_PROJECTIONS {
        let remove = file.entries.len() - MAX_PROJECTIONS;
        file.entries.drain(..remove);
    }
    let bytes = loop {
        let bytes = serde_json::to_vec(&file).map_err(|error| error.to_string())?;
        if bytes.len() as u64 <= MAX_PROJECTION_BYTES {
            break bytes;
        }
        if file.entries.len() <= 1 {
            return Err("one command handle projection exceeds its byte bound".to_owned());
        }
        file.entries.remove(0);
    };
    let pending = root.join(format!("{FILE_NAME}.pending"));
    let mut output = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&pending)
        .map_err(|error| format!("open command projection staging file: {error}"))?;
    output
        .write_all(&bytes)
        .and_then(|()| output.sync_all())
        .map_err(|error| format!("write command projection staging file: {error}"))?;
    drop(output);
    replace(&pending, &path)?;
    #[cfg(unix)]
    fs::File::open(root)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| format!("sync command projection directory: {error}"))?;
    Ok(())
}

pub(super) fn record_deferred(
    root: &Path,
    handle: &str,
    value: Value,
) -> Result<bool, String> {
    let path = root.join(FILE_NAME);
    let file = read_file(&path)?;
    if file.entries.iter().any(|entry| {
        entry.handle == handle
            && entry.value.get("state").and_then(Value::as_str) == Some("completed")
            && entry.value.get("preview_pending").and_then(Value::as_bool) != Some(true)
    }) {
        return Ok(false);
    }
    record(root, handle, value)?;
    Ok(true)
}

fn read_file(path: &Path) -> Result<ProjectionFile, String> {
    match fs::read(path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(file) => Ok(file),
            Err(error) => {
                quarantine(path);
                crate::diagnostic::report(&format!(
                    "peritus command runtime: replaced malformed handle projections {}: {error}",
                    path.display()
                ));
                Ok(ProjectionFile { version: 1, entries: Vec::new() })
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(ProjectionFile { version: 1, entries: Vec::new() })
        }
        Err(error) => Err(format!("read command handle projections: {error}")),
    }
}

fn replace(pending: &Path, path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    if path.exists() {
        fs::remove_file(path)
            .map_err(|error| format!("replace command handle projections: {error}"))?;
    }
    fs::rename(pending, path)
        .map_err(|error| format!("publish command handle projections: {error}"))
}

fn quarantine(path: &Path) {
    let quarantine = path.with_extension("corrupt.json");
    let _ = fs::remove_file(&quarantine);
    if let Err(error) = fs::rename(path, &quarantine) {
        crate::diagnostic::report(&format!(
            "peritus command runtime: could not quarantine malformed handle projections {}: {error}",
            path.display()
        ));
    }
}

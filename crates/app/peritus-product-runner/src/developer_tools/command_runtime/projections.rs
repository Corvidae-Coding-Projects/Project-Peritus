//! Bounded durable product projections for reconnect-stable command handles.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write as _,
    path::Path,
};

use super::super::receipt::NativeCommandOwner;
use super::result;
use peritus_types::{ActionId, ProcessId, RunId};

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    owner: Option<OwnerProjection>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct OwnerProjection {
    #[serde(rename = "source_run_id")]
    source_run: String,
    #[serde(rename = "run_id")]
    execution_run: String,
    #[serde(rename = "action_id")]
    action: String,
    #[serde(rename = "process_id")]
    process: String,
}

pub(super) struct RecoveredProjections {
    pub(super) values: BTreeMap<String, Value>,
    pub(super) owners: BTreeMap<String, NativeCommandOwner>,
}

pub(super) fn load(root: &Path) -> Result<RecoveredProjections, String> {
    let path = root.join(FILE_NAME);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(RecoveredProjections { values: BTreeMap::new(), owners: BTreeMap::new() });
        }
        Err(error) => return Err(format!("read command handle projections: {error}")),
    };
    if bytes.len() as u64 > MAX_PROJECTION_BYTES {
        quarantine(&path);
        return Ok(RecoveredProjections { values: BTreeMap::new(), owners: BTreeMap::new() });
    }
    let file: ProjectionFile = match serde_json::from_slice::<ProjectionFile>(&bytes) {
        Ok(file) if matches!(file.version, 1 | 2) && file.entries.len() <= MAX_PROJECTIONS => file,
        Ok(_) | Err(_) => {
            quarantine(&path);
            return Ok(RecoveredProjections { values: BTreeMap::new(), owners: BTreeMap::new() });
        }
    };
    let mut recovered = BTreeMap::new();
    let mut owners = BTreeMap::new();
    for entry in file.entries {
        if entry.handle.is_empty() || entry.handle.len() > 128 {
            continue;
        }
        let owner = entry.owner.as_ref().map(decode_owner).transpose()?;
        if let Some(owner) = owner {
            owners.insert(entry.handle.clone(), owner);
        }
        let value = if entry.value.get("state").and_then(Value::as_str) == Some("running")
            && owner.is_none()
        {
            result::indeterminate(
                &entry.handle,
                "the daemon restarted while this command was active, but its retained projection has no exact native owner link",
            )
        } else {
            entry.value
        };
        recovered.insert(entry.handle, value);
    }
    Ok(RecoveredProjections { values: recovered, owners })
}

pub(super) fn record(root: &Path, handle: &str, value: Value) -> Result<(), String> {
    record_inner(root, handle, value, None)
}

pub(super) fn record_with_owner(
    root: &Path,
    handle: &str,
    value: Value,
    owner: NativeCommandOwner,
) -> Result<(), String> {
    record_inner(root, handle, value, Some(encode_owner(owner)))
}

fn record_inner(
    root: &Path,
    handle: &str,
    value: Value,
    owner: Option<OwnerProjection>,
) -> Result<(), String> {
    let path = root.join(FILE_NAME);
    let mut file = read_file(&path)?;
    let prior_owner = file
        .entries
        .iter()
        .find(|entry| entry.handle == handle)
        .and_then(|entry| entry.owner.clone());
    file.entries.retain(|entry| entry.handle != handle);
    file.entries.push(Projection {
        handle: handle.to_owned(),
        value,
        owner: owner.or(prior_owner),
    });
    file.version = 2;
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
                Ok(ProjectionFile { version: 2, entries: Vec::new() })
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(ProjectionFile { version: 2, entries: Vec::new() })
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

fn encode_owner(owner: NativeCommandOwner) -> OwnerProjection {
    OwnerProjection {
        source_run: id_hex(owner.source_run.as_bytes()),
        execution_run: id_hex(owner.execution_run.as_bytes()),
        action: id_hex(owner.action.as_bytes()),
        process: id_hex(owner.process.as_bytes()),
    }
}

fn decode_owner(owner: &OwnerProjection) -> Result<NativeCommandOwner, String> {
    Ok(NativeCommandOwner {
        source_run: RunId::new(id_bytes(&owner.source_run)?)
            .map_err(|_| "invalid projected source run identity".to_owned())?,
        execution_run: RunId::new(id_bytes(&owner.execution_run)?)
            .map_err(|_| "invalid projected process run identity".to_owned())?,
        action: ActionId::new(id_bytes(&owner.action)?)
            .map_err(|_| "invalid projected action identity".to_owned())?,
        process: ProcessId::new(id_bytes(&owner.process)?)
            .map_err(|_| "invalid projected process identity".to_owned())?,
    })
}

fn id_hex(bytes: &[u8; 16]) -> String {
    use core::fmt::Write as _;

    let mut output = String::with_capacity(32);
    for byte in bytes {
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn id_bytes(value: &str) -> Result<[u8; 16], String> {
    if value.len() != 32 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("projected native owner identity is not a 16-byte hex ID".to_owned());
    }
    let mut output = [0_u8; 16];
    for (index, byte) in output.iter_mut().enumerate() {
        let offset = index * 2;
        *byte = u8::from_str_radix(&value[offset..offset + 2], 16)
            .map_err(|_| "projected native owner identity hex is malformed".to_owned())?;
    }
    Ok(output)
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

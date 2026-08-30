//! Candidate-bound source and reviewed-inventory reconciliation.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::Digest as _;

use crate::hex_digest;

use super::error::ControllerError;
use super::plan::SourceCheck;

mod security;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(super) struct SourceObservation {
    check: &'static str,
    files: u64,
    items: u64,
    sha256: String,
}

pub(super) fn run(check: SourceCheck, root: &Path) -> Result<SourceObservation, ControllerError> {
    match check {
        SourceCheck::MigrationRecovery => migration_recovery(root),
        SourceCheck::UnsafeInventory => unsafe_inventory(root),
        SourceCheck::TcbInventory => tcb_inventory(root),
        SourceCheck::ThreatInventory => security::threat(root),
        SourceCheck::ControlInventory => security::control(root),
    }
}

fn migration_recovery(root: &Path) -> Result<SourceObservation, ControllerError> {
    let paths = [
        "docs/release-migration-recovery.md",
        "docs/g0-recovery-runbook.md",
        "docs/g0-shutdown-runbook.md",
        "packaging/README.md",
        "release/README.md",
    ];
    let mut files = Vec::new();
    let mut has_recovery = false;
    let mut has_migration = false;
    for relative in paths {
        let bytes = read(root, relative)?;
        let text = std::str::from_utf8(&bytes)
            .map_err(|_| ControllerError::protocol("migration or recovery guide is not UTF-8"))?;
        let lower = text.to_ascii_lowercase();
        let recovery = lower.contains("recover");
        let migration = lower.contains("migrat") || lower.contains("upgrade");
        if bytes.len() < 200 || !(recovery || migration) {
            return Err(ControllerError::protocol(format!(
                "{relative} lacks substantive migration or recovery guidance"
            )));
        }
        has_recovery |= recovery;
        has_migration |= migration;
        files.push((PathBuf::from(relative), bytes));
    }
    if !has_recovery || !has_migration {
        return Err(ControllerError::protocol(
            "release guides do not jointly cover migration and recovery",
        ));
    }
    Ok(observation("migration-recovery", &files, paths.len()))
}

fn unsafe_inventory(root: &Path) -> Result<SourceObservation, ControllerError> {
    let inventory: UnsafeInventory = parse_toml(root, "security/unsafe-inventory-v1.toml")?;
    let expected =
        ["unsafe-block", "unsafe-function", "unsafe-trait-or-impl", "ffi-build-or-generated"];
    if inventory.schema != "peritus.unsafe-inventory.v1"
        || inventory.owner != "H0"
        || inventory.status != "requires-exact-candidate-reconciliation"
        || inventory.policy.source_roots != ["crates"]
        || !inventory.policy.include_generated
        || !inventory.policy.require_safety_comment
        || !inventory.policy.require_owner
        || !inventory.policy.require_threat_reference
        || !inventory.policy.require_miri_eligibility_disposition
        || !inventory.policy.forbid_undocumented
        || inventory.qualification.probe != "h0.inventory.unsafe"
        || inventory.qualification.ready_condition.is_empty()
        || inventory.category.iter().map(|value| value.id.as_str()).collect::<Vec<_>>() != expected
        || inventory.category.iter().any(|value| value.required_fields.is_empty())
    {
        return Err(ControllerError::protocol("unsafe inventory policy is incomplete or changed"));
    }
    let files = rust_sources(&root.join("crates"))?;
    let occurrences = files.iter().map(|(_, bytes)| count_word(bytes, b"unsafe")).sum::<usize>();
    if occurrences == 0 {
        return Err(ControllerError::protocol("unsafe source scan found no inventory input"));
    }
    Ok(observation("unsafe-inventory", &files, occurrences))
}

fn tcb_inventory(root: &Path) -> Result<SourceObservation, ControllerError> {
    let inventory: TcbInventory = parse_toml(root, "security/tcb-inventory-v1.toml")?;
    let expected = [
        "rust-toolchain",
        "verus-vstd-z3",
        "native-sandbox-backends",
        "process-and-path-backends",
        "secret-backend",
        "release-signing-and-provenance",
        "external-security-review",
    ];
    let ids = inventory.component.iter().map(|value| value.id.as_str()).collect::<Vec<_>>();
    if inventory.schema != "peritus.tcb-inventory.v1"
        || inventory.owner != "H0"
        || inventory.status != "requires-exact-candidate-reconciliation"
        || ids != expected
        || inventory.component.iter().any(|value| {
            value.kind.is_empty() || value.identity_source.is_empty() || value.review.is_empty()
        })
        || inventory.qualification.probe != "h0.inventory.tcb"
        || inventory.qualification.ready_condition.is_empty()
    {
        return Err(ControllerError::protocol("TCB inventory is incomplete or changed"));
    }
    let files =
        inventory_files(root, &["security/tcb-inventory-v1.toml", "verification/trust.toml"])?;
    Ok(observation("tcb-inventory", &files, inventory.component.len()))
}

fn parse_toml<T: for<'de> Deserialize<'de>>(
    root: &Path,
    relative: &str,
) -> Result<T, ControllerError> {
    let bytes = read(root, relative)?;
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| ControllerError::protocol("reviewed inventory is not UTF-8"))?;
    Ok(toml::from_str(text)?)
}

fn inventory_files(
    root: &Path,
    relative: &[&str],
) -> Result<Vec<(PathBuf, Vec<u8>)>, ControllerError> {
    relative.iter().map(|path| Ok((PathBuf::from(path), read(root, path)?))).collect()
}

fn read(root: &Path, relative: &str) -> Result<Vec<u8>, ControllerError> {
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path)
        .map_err(|error| ControllerError::io("inspect candidate file", &path, error))?;
    if !metadata.file_type().is_file() || metadata.len() == 0 || metadata.len() > 8 * 1024 * 1024 {
        return Err(ControllerError::protocol(format!("{relative} is not a bounded regular file")));
    }
    fs::read(&path).map_err(|error| ControllerError::io("read candidate file", &path, error))
}

fn rust_sources(root: &Path) -> Result<Vec<(PathBuf, Vec<u8>)>, ControllerError> {
    let mut pending = vec![root.to_path_buf()];
    let mut paths = Vec::new();
    while let Some(directory) = pending.pop() {
        let entries = fs::read_dir(&directory)
            .map_err(|error| ControllerError::io("enumerate Rust sources", &directory, error))?;
        for entry in entries {
            let entry = entry
                .map_err(|error| ControllerError::io("read source entry", &directory, error))?;
            let file_type = entry.file_type().map_err(|error| {
                ControllerError::io("inspect source entry", &entry.path(), error)
            })?;
            if file_type.is_symlink() {
                return Err(ControllerError::protocol("Rust source tree contains a symlink"));
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file()
                && entry.path().extension().is_some_and(|value| value == "rs")
            {
                paths.push(entry.path());
            }
        }
    }
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let bytes = fs::read(&path)
                .map_err(|error| ControllerError::io("read Rust source", &path, error))?;
            let relative = path
                .strip_prefix(root)
                .map_err(|_| ControllerError::protocol("source escaped crates root"))?
                .to_path_buf();
            Ok((relative, bytes))
        })
        .collect()
}

fn observation(
    check: &'static str,
    files: &[(PathBuf, Vec<u8>)],
    items: usize,
) -> SourceObservation {
    let mut hasher = sha2::Sha256::new();
    for (path, bytes) in files {
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update((bytes.len() as u64).to_be_bytes());
        hasher.update(bytes);
    }
    SourceObservation {
        check,
        files: u64::try_from(files.len()).unwrap_or(u64::MAX),
        items: u64::try_from(items).unwrap_or(u64::MAX),
        sha256: hex_digest(peritus_types::Sha256Digest::new(hasher.finalize().into())),
    }
}

fn count_word(bytes: &[u8], word: &[u8]) -> usize {
    bytes
        .windows(word.len())
        .enumerate()
        .filter(|(index, value)| {
            *value == word
                && (*index == 0 || !identifier(bytes[*index - 1]))
                && (*index + word.len() == bytes.len() || !identifier(bytes[*index + word.len()]))
        })
        .count()
}

const fn identifier(value: u8) -> bool {
    value.is_ascii_alphanumeric() || value == b'_'
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnsafeInventory {
    schema: String,
    owner: String,
    status: String,
    policy: UnsafePolicy,
    category: Vec<UnsafeCategory>,
    qualification: Qualification,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "these booleans are the exact independently reviewed TOML schema, not runtime state"
)]
struct UnsafePolicy {
    source_roots: Vec<String>,
    include_generated: bool,
    require_safety_comment: bool,
    require_owner: bool,
    require_threat_reference: bool,
    require_miri_eligibility_disposition: bool,
    forbid_undocumented: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UnsafeCategory {
    id: String,
    required_fields: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Qualification {
    probe: String,
    ready_condition: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TcbInventory {
    schema: String,
    owner: String,
    status: String,
    component: Vec<TcbComponent>,
    qualification: Qualification,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TcbComponent {
    id: String,
    kind: String,
    identity_source: String,
    review: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reviewed_source_inventory_reconciles_the_checked_in_candidate() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(4).expect("workspace root");
        for check in [
            SourceCheck::MigrationRecovery,
            SourceCheck::UnsafeInventory,
            SourceCheck::TcbInventory,
            SourceCheck::ThreatInventory,
            SourceCheck::ControlInventory,
        ] {
            run(check, root).unwrap_or_else(|error| panic!("{check:?}: {error}"));
        }
    }
}

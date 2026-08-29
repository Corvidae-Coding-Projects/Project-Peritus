//! Locked Cargo graph projection into exact SPDX components.

use std::{
    collections::BTreeMap,
    fs,
    io::Read as _,
    path::{Path, PathBuf},
    process::Command,
};

use peritus_release_artifacts::{BoundedId, Sha256Digest, SpdxComponent};
use serde::Deserialize;
use sha2::{Digest as _, Sha256};

use crate::error::OperatorError;

const MAX_COMPONENT_FILES: usize = 100_000;
type PackageKey = (String, String, Option<String>);
type LockedChecksums = BTreeMap<PackageKey, Sha256Digest>;

#[derive(Deserialize)]
struct CargoMetadata {
    packages: Vec<CargoPackage>,
}

#[derive(Deserialize)]
struct CargoPackage {
    name: String,
    version: String,
    license: Option<String>,
    source: Option<String>,
    manifest_path: PathBuf,
}

#[derive(Deserialize)]
struct CargoLock {
    package: Vec<LockedPackage>,
}

#[derive(Deserialize)]
struct LockedPackage {
    name: String,
    version: String,
    source: Option<String>,
    checksum: Option<String>,
}

pub fn components(root: &Path) -> Result<Vec<SpdxComponent>, OperatorError> {
    let metadata = cargo_metadata(root)?;
    let checksums = locked_checksums(root)?;
    metadata.packages.into_iter().map(|package| component(package, &checksums)).collect()
}

fn cargo_metadata(root: &Path) -> Result<CargoMetadata, OperatorError> {
    let output = Command::new("cargo")
        .current_dir(root)
        .args(["metadata", "--locked", "--format-version", "1"])
        .output()
        .map_err(|error| OperatorError::io("run locked Cargo metadata", root, error))?;
    if !output.status.success() {
        return Err(OperatorError::Command {
            operation: "collect locked Cargo graph",
            status: output.status,
        });
    }
    serde_json::from_slice(&output.stdout).map_err(OperatorError::from)
}

fn locked_checksums(root: &Path) -> Result<LockedChecksums, OperatorError> {
    let path = root.join("Cargo.lock");
    let text = fs::read_to_string(&path)
        .map_err(|error| OperatorError::io("read Cargo lockfile", &path, error))?;
    let lock: CargoLock = toml::from_str(&text)?;
    lock.package
        .into_iter()
        .filter_map(|package| {
            package.checksum.map(|checksum| {
                Sha256Digest::parse(&checksum)
                    .map(|digest| ((package.name, package.version, package.source), digest))
                    .map_err(OperatorError::from)
            })
        })
        .collect()
}

fn component(
    package: CargoPackage,
    checksums: &LockedChecksums,
) -> Result<SpdxComponent, OperatorError> {
    let key = (package.name.clone(), package.version.clone(), package.source.clone());
    let digest = checksums.get(&key).copied().map_or_else(
        || {
            let directory = package
                .manifest_path
                .parent()
                .ok_or_else(|| OperatorError::metadata("Cargo manifest has no parent directory"))?;
            digest_directory(directory)
        },
        Ok,
    )?;
    let id = BoundedId::new(component_id(&package.name, &package.version, digest))?;
    SpdxComponent::new(
        &id,
        package.name,
        package.version,
        "NOASSERTION",
        package.source.unwrap_or_else(|| "NOASSERTION".to_owned()),
        package.license.unwrap_or_else(|| "NOASSERTION".to_owned()),
        digest,
    )
    .map_err(OperatorError::from)
}

fn component_id(name: &str, version: &str, digest: Sha256Digest) -> String {
    let stem: String = format!("cargo-{name}-{version}")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-') {
                character
            } else {
                '-'
            }
        })
        .collect();
    format!("{stem}-{}", &digest.to_hex()[..12])
}

fn digest_directory(root: &Path) -> Result<Sha256Digest, OperatorError> {
    let mut paths = Vec::new();
    collect_paths(root, &mut paths)?;
    paths.sort();
    if paths.is_empty() || paths.len() > MAX_COMPONENT_FILES {
        return Err(OperatorError::metadata(format!(
            "component {} has no files or exceeds the file bound",
            root.display()
        )));
    }
    let mut hasher = Sha256::new();
    hasher.update(b"peritus-component-tree-v1\0");
    for path in paths {
        let relative = path
            .strip_prefix(root)
            .map_err(|_| OperatorError::metadata("component path escaped its source root"))?;
        let relative = relative
            .to_str()
            .ok_or_else(|| OperatorError::metadata("component path is not UTF-8"))?
            .replace('\\', "/");
        hash_field(&mut hasher, relative.as_bytes());
        if path.is_symlink() {
            let target = fs::read_link(&path)
                .map_err(|error| OperatorError::io("read component symlink", &path, error))?
                .to_string_lossy()
                .into_owned();
            hash_field(&mut hasher, target.as_bytes());
        } else {
            hash_file_field(&mut hasher, &path)?;
        }
    }
    Ok(Sha256Digest::from_bytes(hasher.finalize().into()))
}

fn collect_paths(current: &Path, output: &mut Vec<PathBuf>) -> Result<(), OperatorError> {
    let mut entries: Vec<_> = fs::read_dir(current)
        .map_err(|error| OperatorError::io("read component directory", current, error))?
        .collect::<Result<_, _>>()
        .map_err(|error| OperatorError::io("enumerate component directory", current, error))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        if matches!(entry.file_name().to_str(), Some(".git" | "target")) {
            continue;
        }
        let path = entry.path();
        let kind = entry
            .file_type()
            .map_err(|error| OperatorError::io("inspect component entry", &path, error))?;
        if kind.is_dir() {
            collect_paths(&path, output)?;
        } else if kind.is_file() || kind.is_symlink() {
            output.push(path);
            if output.len() > MAX_COMPONENT_FILES {
                return Err(OperatorError::metadata("component source exceeds 100000 files"));
            }
        }
    }
    Ok(())
}

fn hash_file_field(hasher: &mut Sha256, path: &Path) -> Result<(), OperatorError> {
    let mut file = fs::File::open(path)
        .map_err(|error| OperatorError::io("open component source", path, error))?;
    let length = file
        .metadata()
        .map_err(|error| OperatorError::io("inspect component source", path, error))?
        .len();
    hasher.update(length.to_le_bytes());
    let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| OperatorError::io("read component source", path, error))?;
        if count == 0 {
            return Ok(());
        }
        hasher.update(&buffer[..count]);
    }
}

fn hash_field(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
mod tests {
    use super::component_id;
    use peritus_release_artifacts::digest_bytes;

    #[test]
    fn component_identity_is_portable() {
        let id = component_id("name+feature", "1.0.0+meta", digest_bytes(b"crate"));
        assert!(id.starts_with("cargo-name-feature-1.0.0-meta-"));
    }
}

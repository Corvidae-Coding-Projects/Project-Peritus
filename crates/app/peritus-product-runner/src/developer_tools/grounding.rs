//! Deterministic evidence that a model inspected the managed repository.

use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    io::Read as _,
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipts {
    schema: u32,
    scope: [u8; 32],
    root: PathBuf,
    files: BTreeMap<String, String>,
    #[serde(default)]
    root_listing: Option<String>,
    #[serde(default)]
    root_empty: bool,
    #[serde(default)]
    grounding_complete: bool,
}

#[derive(Default)]
pub struct GroundingEvidence {
    scope: [u8; 32],
    list_calls: u32,
    search_calls: u32,
    listed_paths: BTreeSet<PathBuf>,
    read_paths: BTreeSet<PathBuf>,
    mutation_paths: BTreeSet<PathBuf>,
    root_observed_empty: bool,
    grounded: bool,
    root: PathBuf,
    receipt_path: Option<PathBuf>,
    digests: BTreeMap<String, String>,
    reused: usize,
    validation_bytes: u64,
    stale: BTreeSet<String>,
    root_listing: Option<String>,
}

impl GroundingEvidence {
    pub(super) const fn reused_count(&self) -> usize {
        self.reused
    }
    pub(super) const fn validation_bytes(&self) -> u64 {
        self.validation_bytes
    }
    pub(super) fn new(root: PathBuf) -> Self {
        Self { root, ..Self::default() }
    }

    pub(super) fn restore(
        &mut self,
        path: PathBuf,
        scope: [u8; 32],
        allowed: impl Fn(&str) -> bool,
    ) {
        self.scope = scope;
        let root = std::fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone());
        if let Ok(bytes) = std::fs::read(&path)
            && let Ok(saved) = serde_json::from_slice::<Receipts>(&bytes)
            && matches!(saved.schema, 1 | 2)
            && saved.scope == scope
            && saved.root == root
        {
            // Readiness is a completed run/workspace phase. A changed file or directory
            // invalidates that observation, not the fact that startup grounding happened.
            // Schema 1 receipts predate the explicit marker but still record the check.
            self.grounded = saved.grounding_complete
                || (saved.schema == 1 && (saved.root_listing.is_some() || !saved.files.is_empty()));
            if saved.root_listing.is_some() && saved.root_listing == listing_digest(&self.root).ok()
            {
                self.list_calls = 1;
                self.root_observed_empty = saved.root_empty;
                self.grounded |= saved.root_empty;
                self.root_listing = saved.root_listing;
            }
            for (relative, expected) in saved.files {
                if !allowed(&relative) {
                    continue;
                }
                let current = super::path::checked(&self.root, &relative, false)
                    .ok()
                    .and_then(|file| file_digest_and_bytes(&file).ok());
                if let Some((_, bytes)) = &current {
                    self.validation_bytes = self.validation_bytes.saturating_add(*bytes);
                }
                self.digests.insert(relative.clone(), expected.clone());
                if current.as_ref().map(|(digest, _)| digest) == Some(&expected) {
                    self.read_paths.insert(PathBuf::from(&relative));
                    self.grounded = true;
                    self.reused += 1;
                } else {
                    self.stale.insert(relative);
                }
            }
        }
        self.receipt_path = Some(path);
    }

    pub(super) fn guidance(&self) -> String {
        let status = if self.grounded {
            "The single startup grounding check is COMPLETE for this run and workspace. Proceed with the current task; do not restart discovery or perform another grounding pass."
        } else {
            "Perform ONE workspace grounding check, then proceed with the current task. A successful workspace listing or file read completes startup grounding; it does not require reading the repository."
        };
        format!(
            "{status} The host revalidated {} previously inspected file observation(s) against full content hashes and current access policy. A role change, retry, or compaction does not reset grounding. Use the accepted design and retained evidence. Subsequent reads must answer a specific implementation or review question, not extend startup discovery. Changed or missing prior observations: {:?}. Refresh a changed input only when this task needs it. These observations grant no permissions or acceptance credit.",
            self.reused, self.stale
        )
    }

    fn persist(&self) {
        let Some(path) = &self.receipt_path else { return };
        let saved = Receipts {
            schema: 2,
            scope: self.scope,
            root: std::fs::canonicalize(&self.root).unwrap_or_else(|_| self.root.clone()),
            files: self.digests.clone(),
            root_listing: self.root_listing.clone(),
            root_empty: self.root_observed_empty,
            grounding_complete: self.grounded,
        };
        let result =
            serde_json::to_vec(&saved).map_err(|error| error.to_string()).and_then(|bytes| {
                super::effect::atomic_write(path, &bytes).map_err(|error| error.to_string())
            });
        if let Err(error) = result {
            eprintln!(
                "peritus: observation cache was not saved; fresh grounding remains available: {error}"
            );
        }
    }
    pub fn record_list(&mut self, path: &str, entries: usize) {
        self.list_calls = self.list_calls.saturating_add(1);
        self.grounded = true;
        if path.is_empty() || path == "." {
            self.root_observed_empty = entries == 0;
            self.root_listing = listing_digest(&self.root).ok();
        }
        self.persist();
    }

    pub fn required_tool_name(&self) -> Option<&'static str> {
        if self.grounded {
            return None;
        }
        Some("workspace_list")
    }

    pub(super) const fn is_complete(&self) -> bool {
        self.grounded
    }

    pub const fn record_search(&mut self) {
        self.search_calls = self.search_calls.saturating_add(1);
    }

    pub fn record_listed_path(&mut self, path: &str) {
        self.listed_paths.insert(PathBuf::from(path));
    }

    pub fn record_read(&mut self, path: &str, digest: &str) {
        self.grounded = true;
        self.read_paths.insert(PathBuf::from(path));
        self.stale.remove(path);
        self.digests.insert(path.to_owned(), digest.to_owned());
        self.persist();
    }

    pub fn record_mutation(&mut self, path: &str) {
        self.mutation_paths.insert(PathBuf::from(path));
        self.stale.remove(path);
        // The role has the exact successful write/patch postimage. Preserve that observation,
        // but never treat command output or an external modification as a new file read.
        if let Ok(digest) = file_digest(&self.root.join(path)) {
            self.digests.insert(path.to_owned(), digest);
            self.read_paths.insert(PathBuf::from(path));
        } else {
            self.digests.remove(path);
            self.read_paths.remove(&PathBuf::from(path));
        }
        self.persist();
    }

    pub fn ensure_mutation_allowed(&self, path: &str, exists: bool) -> Result<(), String> {
        self.validate().map_err(str::to_owned)?;
        if exists && !self.read_paths.contains(&PathBuf::from(path)) {
            return Err(format!(
                "read the existing target before mutating it: {path}. Call workspace_read with this exact path, then retry the edit. Unchanged host-verified observations are reusable across roles and retries."
            ));
        }
        if exists && self.digests.get(path) != file_digest(&self.root.join(path)).ok().as_ref() {
            return Err(format!(
                "the target changed since its verified observation: {path}. Read this exact file and reconcile the change before editing; do not restart repository discovery."
            ));
        }
        Ok(())
    }

    pub fn ensure_empty_directory_removal_allowed(&self, path: &str) -> Result<(), String> {
        self.validate().map_err(str::to_owned)?;
        if !self.listed_paths.contains(&PathBuf::from(path)) {
            return Err(format!("list the empty directory before removing it: {path}"));
        }
        Ok(())
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.grounded {
            return Ok(());
        }
        Err("perform the single workspace grounding check before proceeding")
    }

    pub fn markdown(&self) -> String {
        let mut text = String::from("\n## Repository grounding evidence\n\n");
        let _ = write!(
            text,
            "- Workspace listings: {}\n- Targeted searches: {}\n",
            self.list_calls, self.search_calls,
        );
        if self.root_observed_empty {
            text.push_str("- The workspace root was observed to be empty.\n");
        }
        if !self.read_paths.is_empty() {
            text.push_str("- Files read:\n");
            for path in &self.read_paths {
                let _ = write!(text, "  - `{}`", path.display());
                text.push('\n');
            }
        }
        let _ = writeln!(text, "- Hash-verified observations reused: {}", self.reused);
        text
    }
}

pub(crate) fn file_digest(path: &Path) -> Result<String, std::io::Error> {
    file_digest_and_bytes(path).map(|(digest, _)| digest)
}

fn file_digest_and_bytes(path: &Path) -> Result<(String, u64), std::io::Error> {
    let mut file = std::fs::File::open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::Error::other("not a regular file"));
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    let mut bytes = 0_u64;
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
        bytes = bytes.saturating_add(count as u64);
    }
    Ok((hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect::<String>(), bytes))
}

fn listing_digest(root: &Path) -> Result<String, std::io::Error> {
    let mut names = std::fs::read_dir(root)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<Result<Vec<_>, _>>()?;
    names.sort();
    let mut hash = Sha256::new();
    for name in names {
        let bytes = name.as_encoded_bytes();
        hash.update(bytes.len().to_le_bytes());
        hash.update(bytes);
    }
    Ok(hash.finalize().iter().map(|byte| format!("{byte:02x}")).collect::<String>())
}

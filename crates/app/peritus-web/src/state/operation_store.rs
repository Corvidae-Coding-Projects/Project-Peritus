//! Per-operation durable records with independent ownership and bounded inspection.

use super::{Operation, Result, hex, problem};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    path::{Path, PathBuf},
};

mod paging;
mod records;
mod filesystem;
mod index;
mod lifecycle;
use records::{read_envelope, read_record, same_optional, same_record};
use filesystem::{create_directories, record_names};

const RECORD_VERSION: u16 = 1;
pub(crate) const PAGE_RECORDS: usize = 128;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    schema_version: u16,
    operation: String,
    record: Operation,
}

#[derive(Clone)]
pub(crate) struct OperationStore {
    root: PathBuf,
    namespace: String,
}

pub(crate) struct OperationOwner {
    store: OperationStore,
    operation: String,
    digest: String,
    lock: File,
}

pub(crate) struct PendingPage {
    pub(crate) operations: Vec<(String, Value)>,
    pub(crate) cursor: Option<String>,
    pub(crate) snapshot: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Publication {
    Inserted,
    Existing,
}

impl OperationStore {
    pub(crate) fn open(state_file: &Path, workspace: &str) -> Result<Self> {
        if workspace.is_empty() {
            return Err(problem("An operation store requires a stable workspace identity"));
        }
        let parent = state_file
            .parent()
            .ok_or_else(|| problem("Workspace state has no directory"))?;
        let namespace = digest(b"peritus/web/workspace-operation-store/v1\0", workspace);
        let root = parent.join("workspace.operations").join(&namespace);
        create_directories(&root)?;
        let store = Self { root, namespace };
        store.recover_index()?;
        Ok(store)
    }

    pub(crate) fn owner(&self, operation: &str) -> Result<OperationOwner> {
        validate_operation(operation)?;
        let digest = digest(b"peritus/web/operation-record/v1\0", operation);
        let lock = self.lock_file(&digest)?;
        fs4::FileExt::lock(&lock).map_err(problem)?;
        Ok(OperationOwner {
            store: self.clone(),
            operation: operation.to_owned(),
            digest,
            lock,
        })
    }

    pub(crate) fn try_owner(&self, operation: &str) -> Result<Option<OperationOwner>> {
        validate_operation(operation)?;
        let digest = digest(b"peritus/web/operation-record/v1\0", operation);
        let lock = self.lock_file(&digest)?;
        match fs4::FileExt::try_lock(&lock) {
            Ok(()) => Ok(Some(OperationOwner { store: self.clone(), operation: operation.to_owned(), digest, lock })),
            Err(fs4::TryLockError::WouldBlock) => Ok(None),
            Err(error) => Err(problem(error)),
        }
    }

    fn lock_file(&self, digest: &str) -> Result<File> {
        Ok(OpenOptions::new().read(true).write(true).create(true).truncate(false)
            .open(self.root.join("locks").join(format!("{digest}.lock")))?)
    }

    pub(crate) fn get(&self, operation: &str) -> Result<Option<Operation>> {
        validate_operation(operation)?;
        let digest = digest(b"peritus/web/operation-record/v1\0", operation);
        self.read_paths(operation, &digest)
    }

    pub(crate) fn import(&self, operations: &BTreeMap<String, Operation>) -> Result<()> {
        for (operation, record) in operations {
            self.owner(operation)?.import(record.clone())?;
        }
        for (operation, expected) in operations {
            let retained = self
                .get(operation)?
                .ok_or_else(|| problem("A migrated operation is missing from its durable store"))?;
            if !same_record(&retained, expected) {
                return Err(problem(
                    "A migrated operation conflicts with the original workspace ledger",
                ));
            }
        }
        Ok(())
    }

    fn read_paths(&self, operation: &str, digest: &str) -> Result<Option<Operation>> {
        loop {
        let pending_path = self.path("pending", digest);
        let settled_path = self.path("settled", digest);
        let pending = read_record(&pending_path, operation)?;
        let settled = read_record(&settled_path, operation)?;
        if !same_optional(&pending, &read_record(&pending_path, operation)?)
            || !same_optional(&settled, &read_record(&settled_path, operation)?) {
            // Publication may replace pending, publish settled and retire pending between these
            // lock-free reads. Retry a changing observation; do not invent a conflicting receipt.
            continue;
        }
        return match (pending, settled) {
            (Some(pending), Some(settled)) if !same_record(&pending, &settled) => Err(problem(
                "The pending and settled records conflict for one operation identity",
            )),
            (_, Some(settled)) => Ok(Some(settled)),
            (Some(pending), None) => Ok(Some(pending)),
            (None, None) => Ok(None),
        };
        }
    }

    fn path(&self, state: &str, digest: &str) -> PathBuf {
        self.root.join(state).join(format!("{digest}.json"))
    }
}

fn validate_operation(operation: &str) -> Result<()> {
    if operation.is_empty() {
        return Err(problem("An operation identity is required"));
    }
    Ok(())
}

fn digest(domain: &[u8], value: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(domain);
    hash.update(value.as_bytes());
    hex(&hash.finalize())
}

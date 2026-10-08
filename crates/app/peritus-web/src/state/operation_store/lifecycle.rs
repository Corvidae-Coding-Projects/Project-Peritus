//! Original-operation lifecycle under one exact per-operation owner.

use super::{Operation, OperationOwner, Publication, Result, problem};
use super::filesystem::{publish_noclobber, publish_replace, sync_directory};
use super::records::{envelope, read_record, same_record};
use crate::error::uncertain;
use serde_json::Value;

impl OperationOwner {
    pub(crate) fn identity(&self) -> &str { &self.operation }

    pub(crate) fn get(&self) -> Result<Option<Operation>> {
        self.store.read_paths(&self.operation, &self.digest)
    }

    pub(crate) fn insert(&self, input: Value) -> Result<Publication> {
        let record = Operation { input, prepared: None, result: None };
        match self.get()? {
            Some(existing) if same_record(&existing, &record) || existing.input == record.input => {
                if existing.result.is_none() {
                    self.store.ensure_pending_index(&self.operation, &self.digest)?;
                }
                Ok(Publication::Existing)
            }
            Some(_) => Err(problem(
                "The original operation identity belongs to different input",
            )),
            None => {
                let _index = self.store.index_owner()?;
                self.publish_new("pending", &record)?;
                self.store.index_pending(&self.operation, &self.digest)?;
                Ok(Publication::Inserted)
            }
        }
    }

    pub(crate) fn retain_prepared(&self, prepared: Value) -> Result<()> {
        let mut record = self
            .get()?
            .ok_or_else(|| problem("Original operation record missing"))?;
        if record.prepared.as_ref().is_some_and(|existing| existing != &prepared) {
            return Err(problem("Original operation execution context changed"));
        }
        let settled = record.result.is_some();
        let retryable_unsubmitted = record.result.as_ref()
            .is_some_and(|value| value["retryable"] == true && value["submitted"] == false);
        if settled && record.prepared.is_none() && !retryable_unsubmitted {
            return Err(problem("A settled operation cannot acquire new execution context"));
        }
        if record.prepared.as_ref() == Some(&prepared) {
            return Ok(());
        }
        record.prepared = Some(prepared);
        if !settled { return self.replace_pending(&record); }
        if !retryable_unsubmitted {
            return Err(problem("A submitted settled operation cannot change execution context"));
        }
        // Exact first preparation is legal on an explicitly retryable, unsubmitted recovery
        // marker. Retire any completed duplicate before changing that marker under its owner.
        let pending = self.store.path("pending", &self.digest);
        let _index = self.store.index_owner()?;
        match std::fs::remove_file(&pending) {
            Ok(()) => sync_directory(pending.parent().expect("pending record parent"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
            Err(error) => return Err(uncertain(error)),
        }
        self.store
            .unindex_pending(&self.operation, &self.digest)
            .map_err(|error| uncertain(error.0))?;
        let settled = self.store.path("settled", &self.digest);
        publish_replace(&self.store.root, &settled, &envelope(&self.operation, record)?)
    }

    pub(crate) fn settle(&self, result: Value) -> Result<()> {
        let _index = self.store.index_owner()?;
        let mut record = self
            .get()?
            .ok_or_else(|| problem("Operation record missing"))?;
        if let Some(existing) = &record.result {
            if existing != &result {
                return Err(problem("The operation identity has a different durable result"));
            }
        } else {
            record.result = Some(result);
            // Publish the completed value in pending first. A crash can therefore leave a
            // duplicate completed record, never an unresolved record beside a settled result.
            self.replace_pending(&record)?;
        }
        self.publish_new("settled", &record)?;
        let pending = self.store.path("pending", &self.digest);
        match std::fs::remove_file(&pending) {
            Ok(()) => sync_directory(pending.parent().expect("pending record parent"))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(uncertain(error)),
        }
        self.store
            .unindex_pending(&self.operation, &self.digest)
            .map_err(|error| uncertain(error.0))?;
        Ok(())
    }

    pub(crate) fn complete_retry(&self, result: Value) -> Result<()> {
        let Some(mut record) = self.get()? else {
            return Err(problem("Operation record missing"));
        };
        let Some(previous) = &record.result else {
            return self.settle(result);
        };
        if previous == &result {
            return self.settle(result);
        }
        if previous["retryable"] != true || previous["submitted"] != false {
            return Err(problem("Only an unsubmitted recovered operation can publish a retry result"));
        }
        let pending = self.store.path("pending", &self.digest);
        if pending.exists() {
            let _index = self.store.index_owner()?;
            // Finish any interrupted marker settlement before replacing the synthetic result.
            // A crash on either side of this cleanup leaves the old retryable result intact.
            std::fs::remove_file(&pending).map_err(uncertain)?;
            sync_directory(pending.parent().expect("pending record parent"))?;
            self.store
                .unindex_pending(&self.operation, &self.digest)
                .map_err(|error| uncertain(error.0))?;
        }
        record.result = Some(result);
        let settled = self.store.path("settled", &self.digest);
        if !settled.exists() {
            return Err(problem("The recovered operation result is missing"));
        }
        // The retry holds this operation's OS owner. Replace only the synthetic, explicitly
        // retryable recovery result; initial identities and submitted effect receipts remain
        // no-clobber publications.
        publish_replace(&self.store.root, &settled, &envelope(&self.operation, record)?)
    }

    pub(crate) fn acknowledge_missing_evidence(&self, result: Value) -> Result<()> {
        let _index = self.store.index_owner()?;
        if self.get()?.is_some() {
            return Err(problem("The operation recovered authoritative evidence during review"));
        }
        if !self.store.has_pending_index(&self.operation, &self.digest)? {
            return Err(problem("Operation not found"));
        }
        let record = Operation {
            input: serde_json::json!({
                "command":"recovery-unknown",
                "evidence":"operation-identity-only"
            }),
            prepared: None,
            result: Some(result),
        };
        self.publish_new("settled", &record)?;
        self.store
            .unindex_pending(&self.operation, &self.digest)
            .map_err(|error| uncertain(error.0))
    }

    pub(super) fn import(&self, record: Operation) -> Result<()> {
        if let Some(existing) = self.get()? {
            if !same_record(&existing, &record) {
                return Err(problem(
                    "The operation store already contains conflicting migrated input",
                ));
            }
        }
        if record.result.is_some() {
            let _index = self.store.index_owner()?;
            self.publish_new("settled", &record)?;
            let pending = self.store.path("pending", &self.digest);
            if pending.exists() {
                std::fs::remove_file(&pending).map_err(uncertain)?;
                sync_directory(pending.parent().expect("pending record parent"))?;
            }
            self.store.unindex_pending(&self.operation, &self.digest)?;
        } else {
            let _index = self.store.index_owner()?;
            self.publish_new("pending", &record)?;
            self.store.index_pending(&self.operation, &self.digest)?;
        }
        Ok(())
    }

    fn replace_pending(&self, record: &Operation) -> Result<()> {
        let path = self.store.path("pending", &self.digest);
        if !path.exists() {
            return Err(problem("The pending operation record is missing"));
        }
        publish_replace(&self.store.root, &path, &envelope(&self.operation, record.clone())?)
    }

    fn publish_new(&self, state: &str, record: &Operation) -> Result<()> {
        let path = self.store.path(state, &self.digest);
        let bytes = envelope(&self.operation, record.clone())?;
        match publish_noclobber(&self.store.root, &path, &bytes) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let existing = read_record(&path, &self.operation)?.ok_or_else(|| {
                    problem("An operation record disappeared during conflict inspection")
                })?;
                if same_record(&existing, record) {
                    Ok(())
                } else {
                    Err(problem(
                        "The operation identity already owns a different durable record",
                    ))
                }
            }
            Err(error) => Err(uncertain(error)),
        }
    }
}

impl Drop for OperationOwner {
    fn drop(&mut self) {
        if let Err(error) = fs4::FileExt::unlock(&self.lock) {
            eprintln!("peritus web: operation ownership unlock failed: {error}");
        }
    }
}

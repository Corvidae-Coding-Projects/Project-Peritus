//! A pending stage borrows its job exclusively and owns its declared input scope.
use super::Job;
use crate::{Definition, Error, Role, Stage, invalid};
use serde_json::Value;

/// One exact uncommitted stage, its role, and its allowed predecessor reads.
///
/// A stage cannot be constructed independently of its manifest/checkpoint. Its exclusive
/// job borrow prevents another stage from advancing while inference is in flight. Dropping
/// it leaves the checkpoint unchanged; completion consumes it and publishes durably.
pub struct PendingStage<'job> {
    job: &'job mut Job,
    index: usize,
    role: usize,
}

impl Job {
    /// Resolves the next stage and role from this job's exact retained package.
    ///
    /// # Errors
    /// Rejects a missing role rather than asking the execution bridge to reconstruct one.
    pub fn pending_stage(&mut self) -> Result<Option<PendingStage<'_>>, Error> {
        let index = self.state.completed.len();
        let Some(stage) = self.definition.manifest.stages.get(index) else { return Ok(None) };
        let role = self
            .definition
            .manifest
            .roles
            .iter()
            .position(|r| r.id == stage.role)
            .ok_or_else(|| invalid("module stage role is missing"))?;
        Ok(Some(PendingStage { job: self, index, role }))
    }
}

impl PendingStage<'_> {
    /// Manifest declaration for this exact stage.
    #[must_use]
    pub fn spec(&self) -> &Stage {
        &self.job.definition.manifest.stages[self.index]
    }
    /// Resolved role from the same manifest.
    #[must_use]
    pub fn role(&self) -> &Role {
        &self.job.definition.manifest.roles[self.role]
    }
    /// Exact package metadata for progress and immutable resource selection.
    #[must_use]
    pub const fn definition(&self) -> &Definition {
        &self.job.definition
    }
    /// Original governing task, independent of later conversation inputs.
    #[must_use]
    pub fn task(&self) -> &str {
        &self.job.state.task
    }
    /// Governing input revision used to name stage requests.
    #[must_use]
    pub const fn input_revision(&self) -> u64 {
        self.job.state.input_revision
    }
    /// Stable job identity without exposing its on-disk layout to callers.
    #[must_use]
    pub fn namespace(&self) -> &str {
        &self.job.namespace
    }

    /// Reads an integrity-checked page only from this stage's declared predecessors.
    ///
    /// # Errors
    /// Rejects undeclared, uncommitted, corrupt, or invalidly paged outputs.
    pub fn read_input(&self, id: &str, offset: u64, max_bytes: usize) -> Result<Value, Error> {
        self.job.read(&self.spec().inputs, id, offset, max_bytes)
    }

    /// Publishes this exact stage and only then advances the durable checkpoint.
    ///
    /// # Errors
    /// Rejects empty output or failed durable publication; prior outputs remain retained.
    pub fn complete(self, text: &str) -> Result<(), Error> {
        let id = self.spec().id.clone();
        self.job.complete(&id, text)
    }
}

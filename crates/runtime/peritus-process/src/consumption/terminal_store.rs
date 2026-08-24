//! Durable terminal-result and artifact-publication operations.

use peritus_types::ProcessId;

use crate::{
    ErrorCode, LifecyclePhase, OutputArtifact, ProcessError, ProcessOperation, RecoveryClass,
    TerminalResult, registry_storage::write_manifest, terminal::terminal_digest,
};

use super::{ProcessStore, store_error};

impl ProcessStore {
    pub(crate) fn record_terminal(
        &self,
        process_id: ProcessId,
        result: &TerminalResult,
    ) -> Result<(), ProcessError> {
        let digest = terminal_digest(result)?;
        self.update(process_id, |manifest| {
            if manifest.phase != LifecyclePhase::Closed
                || result.process_id() != process_id
                || result.plan_digest() != manifest.plan_digest
                || !manifest.matches_terminal(result)
            {
                return Err(store_error(
                    "terminal process result is out of sequence or mismatched",
                ));
            }
            manifest.terminal_digest = Some(digest);
            manifest.terminal = Some(result.clone());
            manifest.phase = LifecyclePhase::Terminal;
            Ok(())
        })
    }

    /// Returns the complete terminal result persisted for one process.
    ///
    /// # Errors
    ///
    /// Returns a typed recovery error when the process is missing or has no complete terminal
    /// record.
    pub fn terminal_result(&self, process_id: ProcessId) -> Result<TerminalResult, ProcessError> {
        self.lock_state()
            .manifests
            .get(&process_id)
            .and_then(|manifest| manifest.terminal.clone())
            .ok_or_else(|| terminal_unavailable("complete terminal result is not persisted"))
    }

    /// Atomically records one finalized output artifact in the durable terminal result.
    ///
    /// Repeating the exact stream artifact is idempotent. A different artifact for an already
    /// published stream, or declaring completion before every retained stream is represented, is
    /// rejected.
    ///
    /// # Errors
    ///
    /// Returns a typed artifact or persistence error for missing terminal state, conflicting
    /// publication, incomplete completion, or failed durable replacement.
    pub(crate) fn record_artifact_publication(
        &self,
        process_id: ProcessId,
        artifact: OutputArtifact,
        complete: bool,
    ) -> Result<TerminalResult, ProcessError> {
        let mut state = self.lock_state();
        let manifest = state
            .manifests
            .get_mut(&process_id)
            .ok_or_else(|| terminal_unavailable("process manifest is missing"))?;
        let mut result = manifest
            .terminal
            .clone()
            .ok_or_else(|| terminal_unavailable("complete terminal result is not persisted"))?;
        validate_artifact(&result, artifact)?;
        match result.artifacts().iter().find(|existing| existing.stream() == artifact.stream()) {
            Some(existing) if *existing != artifact => {
                return Err(artifact_error("published artifact conflicts with the durable stream"));
            }
            Some(_) => {}
            None => result.add_artifact(artifact),
        }
        if complete {
            if !all_retained_streams_published(&result) {
                return Err(artifact_error("artifact completion omits a retained output stream"));
            }
            result.mark_artifacts_complete();
        } else if !result.artifact_publication_complete() {
            result.mark_artifact_failure();
        }
        let digest = terminal_digest(&result)?;
        let mut next = manifest.clone();
        next.terminal_digest = Some(digest);
        next.terminal = Some(result.clone());
        write_manifest(&self.inner.manifests, &next)?;
        *manifest = next;
        drop(state);
        Ok(result)
    }

    pub(crate) fn complete_artifact_publication(
        &self,
        process_id: ProcessId,
    ) -> Result<TerminalResult, ProcessError> {
        let mut state = self.lock_state();
        let manifest = state
            .manifests
            .get_mut(&process_id)
            .ok_or_else(|| terminal_unavailable("process manifest is missing"))?;
        let mut result = manifest
            .terminal
            .clone()
            .ok_or_else(|| terminal_unavailable("complete terminal result is not persisted"))?;
        if !all_retained_streams_published(&result) {
            return Err(artifact_error("artifact completion omits a retained output stream"));
        }
        if result.artifact_publication_complete() {
            return Ok(result);
        }
        result.mark_artifacts_complete();
        let digest = terminal_digest(&result)?;
        let mut next = manifest.clone();
        next.terminal_digest = Some(digest);
        next.terminal = Some(result.clone());
        write_manifest(&self.inner.manifests, &next)?;
        *manifest = next;
        drop(state);
        Ok(result)
    }
}

fn all_retained_streams_published(result: &TerminalResult) -> bool {
    result.output().streams().iter().filter(|stream| stream.retained() > 0).all(|stream| {
        result.artifacts().iter().any(|artifact| artifact.stream() == stream.stream())
    })
}

fn validate_artifact(
    result: &TerminalResult,
    artifact: OutputArtifact,
) -> Result<(), ProcessError> {
    let Some(stream) =
        result.output().streams().iter().find(|stream| stream.stream() == artifact.stream())
    else {
        return Err(artifact_error("published artifact has no terminal output stream"));
    };
    if artifact.start_offset() != 0
        || artifact.end_offset() != artifact.size()
        || artifact.size() != stream.retained()
        || artifact.completeness() != stream.completeness()
    {
        return Err(artifact_error("published artifact differs from terminal output accounting"));
    }
    Ok(())
}

const fn terminal_unavailable(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Indeterminate,
        ProcessOperation::Reconcile,
        RecoveryClass::ReopenAndReconcile,
        detail,
    )
}

const fn artifact_error(detail: &'static str) -> ProcessError {
    ProcessError::new(
        ErrorCode::Artifact,
        ProcessOperation::PublishArtifact,
        RecoveryClass::RetryPublication,
        detail,
    )
}

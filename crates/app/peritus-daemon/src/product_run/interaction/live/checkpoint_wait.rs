//! Recoverable storage waiting retains the admitted tool and its live provider context.

use super::{LiveConversation, ProductActivityKind, WorkspaceMutationKind};
use crate::product_run::publication::{MutationDisposition, RunMutationKind};
use peritus_product_runner::ConversationView;
use std::{path::Path, sync::atomic::Ordering, time::Duration};

const CHECKPOINT_WAIT_STATUS: &str = "Waiting for storage to save a checkpoint";

impl LiveConversation {
    pub(super) async fn capture_checkpoint_when_available(
        &self,
        path: &Path,
        kind: WorkspaceMutationKind,
    ) -> Result<(), String> {
        let start = self.workbench_start_record().map_err(|error| error.to_string())?;
        let (cancelled, cancellation, input_revision) = {
            let records =
                self.service.inner.records.read().map_err(|_| "run registry unavailable")?;
            let record = records.get(&self.run_id).ok_or("run no longer available")?;
            (
                record.cancelled.clone(),
                record.provider_cancellation.clone(),
                record.interaction.incorporated,
            )
        };
        let mut waiting = None;
        let result = loop {
            if cancelled.load(Ordering::Acquire) || cancellation.is_cancelled() {
                break Err("checkpoint preflight cancelled before workspace mutation".to_owned());
            }
            if self.revision() != input_revision {
                break Err("new user input superseded the pending workspace mutation".to_owned());
            }
            let service = self.service.clone();
            let run = self.run_id;
            let start = start.clone();
            let path = path.to_path_buf();
            // The owned job is joined even during cancellation: no orphan capture can publish
            // later, and the Tokio thread remains available for UI, input and cancellation.
            let captured = match tokio::task::spawn_blocking(move || {
                service.capture_automatic_checkpoint(&start, run, &path, kind)
            })
            .await
            {
                Ok(captured) => captured,
                Err(_) => break Err("checkpoint capture worker failed".to_owned()),
            };
            match captured {
                Ok(()) => {
                    if cancelled.load(Ordering::Acquire) || cancellation.is_cancelled() {
                        break Err(
                            "checkpoint preflight cancelled before workspace mutation".to_owned()
                        );
                    }
                    break Ok(());
                }
                Err(error) if error.is_storage_exhausted() => {
                    if waiting.is_none() {
                        waiting = Some(self.checkpoint_wait_status()?);
                    }
                }
                Err(error) => break Err(format!("checkpoint preflight failed: {error}")),
            }
            tokio::select! {
                () = cancellation.cancelled() => {}
                () = tokio::time::sleep(Duration::from_secs(1)) => {}
            }
        };
        if let Some(status) = waiting {
            self.checkpoint_wait_finished(&status, result.is_ok())?;
        }
        result
    }

    fn checkpoint_wait_status(&self) -> Result<String, String> {
        let identity = self
            .service
            .capture_run_identity(self.run_id)
            .map_err(|error| error.to_string())?;
        if !std::sync::Arc::ptr_eq(&identity.cancelled, &self.attempt_cancelled) {
            return Err("run attempt changed before checkpoint storage wait".to_owned());
        }
        let previous = identity.snapshot.status().to_owned();
        let mut input = b"peritus-product-run-checkpoint-wait-enter-v1\0".to_vec();
        input.extend_from_slice(self.run_id.as_bytes());
        input.extend_from_slice(&(previous.len() as u64).to_be_bytes());
        input.extend_from_slice(previous.as_bytes());
        let (previous, _ticket) = self
            .service
            .mutate_run(
                self.run_id,
                Some(&self.attempt_cancelled),
                RunMutationKind::InteractionActivity,
                peritus_codec::sha256(&input),
                MutationDisposition::Observation,
                move |record| {
                    if record.request.workspace_id() != identity.workspace
                        || record.interaction.workbench != identity.start
                        || record.snapshot.status() != previous
                    {
                        return Err(crate::product_run::ProductRunServiceError::InvalidState);
                    }
                    record
                        .interaction
                        .append(
                            ProductActivityKind::Status,
                            CHECKPOINT_WAIT_STATUS,
                            "Free storage to resume. The current task and conversation remain active; you can cancel.",
                        )?;
                    record.progress.mark_event("waiting for checkpoint storage");
                    record.snapshot = crate::product_run::replace_snapshot(
                        &record.snapshot,
                        record.snapshot.phase(),
                        CHECKPOINT_WAIT_STATUS,
                        record.snapshot.summary(),
                    )?;
                    Ok(previous)
                },
            )
            .map_err(|error| error.to_string())?;
        // This status must be visible when the very resource needed for persistence is full.
        // The admitted tool and accepted checkpoints remain owned by the durable journal.
        Ok(previous)
    }

    fn checkpoint_wait_finished(&self, previous: &str, resumed: bool) -> Result<(), String> {
        let previous = previous.to_owned();
        let mut input = b"peritus-product-run-checkpoint-wait-exit-v1\0".to_vec();
        input.extend_from_slice(self.run_id.as_bytes());
        input.push(u8::from(resumed));
        input.extend_from_slice(&(previous.len() as u64).to_be_bytes());
        input.extend_from_slice(previous.as_bytes());
        let (_, _ticket) = self
            .service
            .mutate_run(
                self.run_id,
                Some(&self.attempt_cancelled),
                RunMutationKind::InteractionActivity,
                peritus_codec::sha256(&input),
                MutationDisposition::Observation,
                move |record| {
                    if record.snapshot.status() != CHECKPOINT_WAIT_STATUS {
                        return Err(crate::product_run::ProductRunServiceError::InvalidState);
                    }
                    record.interaction.append(
                        ProductActivityKind::Status,
                        if resumed {
                            "Checkpoint saved; resuming the pending tool"
                        } else {
                            "Checkpoint wait ended before workspace mutation"
                        },
                        "",
                    )?;
                    record.progress.mark_event(if resumed {
                        "checkpoint storage available"
                    } else {
                        "checkpoint preflight stopped"
                    });
                    record.snapshot = crate::product_run::replace_snapshot(
                        &record.snapshot,
                        record.snapshot.phase(),
                        &previous,
                        record.snapshot.summary(),
                    )?;
                    Ok(())
                },
            )
            .map_err(|error| error.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
impl crate::product_run::ProductRunService {
    pub(crate) async fn capture_checkpoint_when_available_for_test(
        &self,
        run_id: peritus_types::RunId,
        path: &Path,
        kind: WorkspaceMutationKind,
    ) -> Result<(), String> {
        LiveConversation::open(self.clone(), run_id)
            .map_err(|error| error.to_string())?
            .capture_checkpoint_when_available(path, kind)
            .await
    }
}

#[cfg(test)]
mod tests {
    use crate::product_control::ControlStoreError;
    use std::io;

    #[test]
    fn waits_only_for_actual_storage_exhaustion() {
        let sqlite_full = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_FULL),
            None,
        );
        assert!(ControlStoreError::Io(io::Error::other(sqlite_full)).is_storage_exhausted());
        assert!(
            ControlStoreError::Io(io::Error::from(io::ErrorKind::StorageFull))
                .is_storage_exhausted()
        );
        assert!(
            ControlStoreError::Io(io::Error::from(io::ErrorKind::QuotaExceeded))
                .is_storage_exhausted()
        );
        assert!(
            !ControlStoreError::Io(io::Error::from(io::ErrorKind::WouldBlock))
                .is_storage_exhausted()
        );
        assert!(
            !ControlStoreError::Io(io::Error::from(io::ErrorKind::PermissionDenied))
                .is_storage_exhausted()
        );
        assert!(!ControlStoreError::StalePreimage.is_storage_exhausted());
        assert!(!ControlStoreError::Corrupt("snapshot checksum").is_storage_exhausted());
    }
}

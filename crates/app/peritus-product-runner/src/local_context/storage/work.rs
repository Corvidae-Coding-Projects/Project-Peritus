//! Content-free operation counters for the local context store, never durable authority.

use serde::Serialize;

#[derive(Clone, Copy, Default, Serialize)]
pub(in crate::local_context) struct StorageWork {
    pub(super) store_calls: u64,
    pub(super) submitted_bytes: u64,
    pub(super) write_attempts: u64,
    pub(super) completed_write_bytes: u64,
    pub(super) finalizations: u64,
    pub(super) reuse_attempts: u64,
    pub(super) reused_objects: u64,
    pub(super) verified_bytes: u64,
    pub(super) read_calls: u64,
    pub(super) completed_read_bytes: u64,
    pub(super) elapsed_micros: u64,
}

impl StorageWork {
    pub(in crate::local_context) const fn since(self, previous: Self) -> Self {
        Self {
            store_calls: self.store_calls.saturating_sub(previous.store_calls),
            submitted_bytes: self.submitted_bytes.saturating_sub(previous.submitted_bytes),
            write_attempts: self.write_attempts.saturating_sub(previous.write_attempts),
            completed_write_bytes: self
                .completed_write_bytes
                .saturating_sub(previous.completed_write_bytes),
            finalizations: self.finalizations.saturating_sub(previous.finalizations),
            reuse_attempts: self.reuse_attempts.saturating_sub(previous.reuse_attempts),
            reused_objects: self.reused_objects.saturating_sub(previous.reused_objects),
            verified_bytes: self.verified_bytes.saturating_sub(previous.verified_bytes),
            read_calls: self.read_calls.saturating_sub(previous.read_calls),
            completed_read_bytes: self
                .completed_read_bytes
                .saturating_sub(previous.completed_read_bytes),
            elapsed_micros: self.elapsed_micros.saturating_sub(previous.elapsed_micros),
        }
    }
}

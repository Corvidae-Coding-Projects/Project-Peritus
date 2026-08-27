//! Fixed memory, process, attachment, replay, and delivery bounds.

/// Operational limits for the live terminal registry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalRegistryLimits {
    pub(super) maximum_processes: usize,
    pub(super) maximum_attachments: usize,
    maximum_attachments_per_process: usize,
    maximum_replay_events_per_process: usize,
    maximum_replay_bytes_per_process: usize,
    maximum_pending_events_per_attachment: usize,
    maximum_pending_bytes_per_attachment: usize,
    maximum_process_events_per_page: usize,
    maximum_process_pages_per_poll: usize,
    maximum_delivery_events_per_poll: usize,
}

impl TerminalRegistryLimits {
    /// Production terminal bridge limits.
    pub(crate) const PRODUCTION: Self = Self {
        maximum_processes: 1_024,
        maximum_attachments: 4_096,
        maximum_attachments_per_process: 8,
        maximum_replay_events_per_process: 4_096,
        maximum_replay_bytes_per_process: 8 * 1_024 * 1_024,
        maximum_pending_events_per_attachment: 1_024,
        maximum_pending_bytes_per_attachment: 2 * 1_024 * 1_024,
        maximum_process_events_per_page: 256,
        maximum_process_pages_per_poll: 8,
        maximum_delivery_events_per_poll: 128,
    };

    pub(super) const fn maximum_attachments_per_process(self) -> usize {
        self.maximum_attachments_per_process
    }
    pub(super) const fn maximum_replay_events_per_process(self) -> usize {
        self.maximum_replay_events_per_process
    }
    pub(super) const fn maximum_replay_bytes_per_process(self) -> usize {
        self.maximum_replay_bytes_per_process
    }
    pub(super) const fn maximum_pending_events_per_attachment(self) -> usize {
        self.maximum_pending_events_per_attachment
    }
    pub(super) const fn maximum_pending_bytes_per_attachment(self) -> usize {
        self.maximum_pending_bytes_per_attachment
    }
    pub(super) const fn maximum_process_events_per_page(self) -> usize {
        self.maximum_process_events_per_page
    }
    pub(super) const fn maximum_process_pages_per_poll(self) -> usize {
        self.maximum_process_pages_per_poll
    }
    pub(super) const fn maximum_delivery_events_per_poll(self) -> usize {
        self.maximum_delivery_events_per_poll
    }

    pub(super) const fn valid(self) -> bool {
        self.maximum_processes > 0
            && self.maximum_attachments > 0
            && self.maximum_attachments_per_process > 0
            && self.maximum_attachments_per_process <= self.maximum_attachments
            && self.maximum_replay_events_per_process > 0
            && self.maximum_replay_bytes_per_process > 0
            && self.maximum_pending_events_per_attachment > 0
            && self.maximum_pending_bytes_per_attachment > 0
            && self.maximum_process_events_per_page > 0
            && self.maximum_process_pages_per_poll > 0
            && self.maximum_delivery_events_per_poll > 0
            && self.maximum_delivery_events_per_poll <= self.maximum_pending_events_per_attachment
    }
}

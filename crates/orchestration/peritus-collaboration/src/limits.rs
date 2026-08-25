//! Independently checked collaboration bounds.

use crate::error::{CollaborationError, CollaborationErrorKind, reject};

/// Complete immutable bounds for one collaboration aggregate.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CollaborationLimits {
    tasks: u32,
    depth: u16,
    fan_out: u16,
    messages: u32,
    recipients: u16,
    payload_bytes: u32,
    artifact_references: u16,
    command_bytes: u64,
    state_bytes: u64,
}

impl CollaborationLimits {
    /// Maximum retained tasks.
    pub const MAX_TASKS: u32 = 65_535;
    /// Maximum causal depth.
    pub const MAX_DEPTH: u16 = 256;
    /// Maximum direct children per task.
    pub const MAX_FAN_OUT: u16 = 4_096;
    /// Maximum retained messages.
    pub const MAX_MESSAGES: u32 = 262_144;
    /// Maximum distinct recipients in one task.
    pub const MAX_RECIPIENTS: u16 = 4_096;
    /// Maximum inert payload bytes described by one message.
    pub const MAX_PAYLOAD_BYTES: u32 = 1_048_576;
    /// Maximum artifact references retained by one task.
    pub const MAX_ARTIFACT_REFERENCES: u16 = 4_096;
    /// Maximum canonical bytes in one command.
    pub const MAX_COMMAND_BYTES: u64 = 16 * 1_048_576 - 16;
    /// Maximum canonical bytes in complete state.
    pub const MAX_STATE_BYTES: u64 = 64 * 1_048_576 - 16;

    /// Creates independently checked limits.
    ///
    /// # Errors
    /// Rejects zero values and values above compiled production ceilings.
    #[allow(clippy::too_many_arguments, reason = "independent allocation bounds stay explicit")]
    pub fn new(
        tasks: u32,
        depth: u16,
        fan_out: u16,
        messages: u32,
        recipients: u16,
        payload_bytes: u32,
        artifact_references: u16,
        command_bytes: u64,
        state_bytes: u64,
    ) -> Result<Self, CollaborationError> {
        let values = [
            (u64::from(tasks), u64::from(Self::MAX_TASKS)),
            (u64::from(depth), u64::from(Self::MAX_DEPTH)),
            (u64::from(fan_out), u64::from(Self::MAX_FAN_OUT)),
            (u64::from(messages), u64::from(Self::MAX_MESSAGES)),
            (u64::from(recipients), u64::from(Self::MAX_RECIPIENTS)),
            (u64::from(payload_bytes), u64::from(Self::MAX_PAYLOAD_BYTES)),
            (u64::from(artifact_references), u64::from(Self::MAX_ARTIFACT_REFERENCES)),
            (command_bytes, Self::MAX_COMMAND_BYTES),
            (state_bytes, Self::MAX_STATE_BYTES),
        ];
        if values.into_iter().any(|(value, ceiling)| value == 0 || value > ceiling) {
            return Err(reject(
                CollaborationErrorKind::InvalidLimit,
                "collaboration limit is zero or exceeds its production ceiling",
            ));
        }
        Ok(Self::from_wire(
            tasks,
            depth,
            fan_out,
            messages,
            recipients,
            payload_bytes,
            artifact_references,
            command_bytes,
            state_bytes,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) const fn from_wire(
        tasks: u32,
        depth: u16,
        fan_out: u16,
        messages: u32,
        recipients: u16,
        payload_bytes: u32,
        artifact_references: u16,
        command_bytes: u64,
        state_bytes: u64,
    ) -> Self {
        Self {
            tasks,
            depth,
            fan_out,
            messages,
            recipients,
            payload_bytes,
            artifact_references,
            command_bytes,
            state_bytes,
        }
    }

    /// Maximum retained tasks.
    #[must_use]
    pub const fn tasks(self) -> u32 {
        self.tasks
    }
    /// Maximum task depth.
    #[must_use]
    pub const fn depth(self) -> u16 {
        self.depth
    }
    /// Maximum direct fan-out.
    #[must_use]
    pub const fn fan_out(self) -> u16 {
        self.fan_out
    }
    /// Maximum retained messages.
    #[must_use]
    pub const fn messages(self) -> u32 {
        self.messages
    }
    /// Maximum distinct task recipients.
    #[must_use]
    pub const fn recipients(self) -> u16 {
        self.recipients
    }
    /// Maximum message payload bytes.
    #[must_use]
    pub const fn payload_bytes(self) -> u32 {
        self.payload_bytes
    }
    /// Maximum artifact references per task.
    #[must_use]
    pub const fn artifact_references(self) -> u16 {
        self.artifact_references
    }
    /// Maximum canonical command bytes.
    #[must_use]
    pub const fn command_bytes(self) -> u64 {
        self.command_bytes
    }
    /// Maximum complete state bytes.
    #[must_use]
    pub const fn state_bytes(self) -> u64 {
        self.state_bytes
    }
}

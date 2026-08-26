//! Nested application-protocol metadata grouped by stable concern.

use super::AppTypeDescriptor;

mod artifact_prompt;
mod command_subscription;
mod protocol;

/// Nested metadata groups in dependency-before-consumer order.
pub const APP_NESTED_TYPES: &[&[AppTypeDescriptor]] = &[
    protocol::HANDSHAKE_TYPES,
    protocol::VERSION_LIMIT_TYPES,
    command_subscription::COMMAND_TYPES,
    command_subscription::DELIVERY_TYPES,
    command_subscription::CONTROL_TYPES,
    artifact_prompt::ARTIFACT_PROMPT_TYPES,
];

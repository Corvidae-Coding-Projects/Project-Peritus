//! Stateful application-protocol metadata grouped by stable concern.

use super::AppTypeDescriptor;

mod daemon;
mod prompt_terminal;

/// Stateful flow metadata groups in dependency-before-consumer order.
pub const APP_FLOW_TYPES: &[&[AppTypeDescriptor]] =
    &[prompt_terminal::PROMPT_TERMINAL_TYPES, daemon::DAEMON_TYPES];

//! Live C2 process control projected through bounded A3 terminal attachments.

mod attachment;
mod bridge;
mod error;
mod limits;
mod recovery;
mod registry;

pub(crate) use error::{TerminalBridgeError, TerminalBridgeErrorKind};
pub(crate) use limits::TerminalRegistryLimits;
pub(crate) use registry::{TerminalBridgeEvent, TerminalRegistry};

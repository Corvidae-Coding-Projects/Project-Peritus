//! Live C2 process control projected through bounded A3 terminal attachments.

mod attachment;
mod bridge;
mod error;
mod limits;
mod qualification;
mod recovery;
mod registry;

pub use error::{TerminalBridgeError, TerminalBridgeErrorKind};
pub use limits::TerminalRegistryLimits;
pub use qualification::qualify_pty_ordering;
pub use registry::{TerminalBridgeEvent, TerminalRegistry};

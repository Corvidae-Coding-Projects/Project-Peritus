//! Honest daemon readiness, heartbeat, and shutdown observations.

mod error;
mod heartbeat;
mod shutdown;
mod status;

pub use error::{DaemonControlError, DaemonControlErrorKind};
pub use heartbeat::{DaemonHeartbeat, HeartbeatState};
pub use shutdown::{
    RemainingWork, RemainingWorkKind, ShutdownAccepted, ShutdownComplete,
    ShutdownCompletionDisposition, ShutdownPhase, ShutdownProgress, ShutdownRequest, ShutdownState,
};
pub use status::{DaemonReadiness, DaemonStatus};

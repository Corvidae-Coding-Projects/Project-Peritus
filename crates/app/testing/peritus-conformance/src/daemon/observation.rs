//! Direct black-box observations returned by a G0 daemon adapter.

mod admission;
mod lifecycle;
mod services;

pub use admission::*;
pub use lifecycle::*;
pub use services::*;

/// One scenario-specific observation collected through the production daemon boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DaemonConformanceObservation {
    /// Session negotiation, peer binding, or negotiated-context facts.
    Session(DaemonSessionObservation),
    /// Durable command admission, settlement, and replay facts.
    Command(DaemonCommandObservation),
    /// Subscription delivery, acknowledgement, gap, or pressure facts.
    Subscription(DaemonSubscriptionObservation),
    /// Artifact transfer and publication facts.
    Artifact(DaemonArtifactObservation),
    /// Prompt authority and settlement facts.
    Prompt(DaemonPromptObservation),
    /// Combined PTY stream and terminal settlement facts.
    Terminal(DaemonTerminalObservation),
    /// Readiness-specific request admission facts.
    Admission(DaemonAdmissionObservation),
    /// Exclusive instance ownership facts.
    Instance(DaemonInstanceObservation),
    /// Startup failure and recovery facts.
    Startup(DaemonStartupObservation),
    /// Outbox effect reconciliation and acknowledgement facts.
    Outbox(DaemonOutboxObservation),
    /// Graceful shutdown facts.
    Shutdown(DaemonShutdownObservation),
    /// Forced-restart recovery facts.
    Recovery(DaemonRecoveryObservation),
    /// Resource-bound enforcement facts.
    Bounds(DaemonBoundsObservation),
    /// Framing rejection and preallocation facts.
    Frame(DaemonFrameObservation),
    /// Non-authority surface facts.
    NonAuthority(DaemonNonAuthorityObservation),
}

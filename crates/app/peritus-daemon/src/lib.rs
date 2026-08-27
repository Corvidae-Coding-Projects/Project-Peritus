//! Production daemon and application composition owner for Peritus.
//!
//! One serialized authority task owns writable state. IPC connections, subscriptions, providers,
//! tools, and background workers exchange bounded typed messages with that owner and never receive
//! a writable journal handle.

mod artifact;
mod authority;
mod command;
mod component;
mod config;
mod domain;
mod error;
mod identity;
mod instance;
mod ipc;
mod lifecycle;
mod outbox;
mod prompt;
mod session;
mod shutdown;
mod startup;
mod subscription;
mod telemetry;
mod terminal;
pub mod verified;
mod worker;

pub use authority::{AuthorityHandle, AuthorityOwner};
pub use component::{
    DaemonComponents, DispatcherBinding, FilesystemDispatcherRoute, GitDispatcherRoute,
    OfficialExecutableSelection, ProviderAdapterKind, ProviderDeclaration, ProviderProfileKey,
    ProviderRegistry, ProviderRegistryError, ProviderRegistryErrorKind, ProviderRegistryLimits,
    ToolComponentError, ToolComponentErrorKind, ToolComponents, ToolDispatcherRoute,
    ToolRegistration,
};
pub use config::{
    DaemonConfig, DaemonLimits, DaemonPaths, LocalHumanPrincipal, ProjectDeclaration,
    ProviderProfileDeclaration, ProviderRoute, ProviderRouteKind, TelemetryExport, ToolPolicy,
    WorkspaceDeclaration,
};
pub use error::{DaemonError, DaemonErrorCode, DaemonRecovery};
pub use identity::DaemonIdentity;
pub use ipc::{
    AppFrameStream, AuthenticatedConnection, LocalEndpoint, LocalEndpointAddress, PeerIdentity,
};
pub use lifecycle::{DaemonLifecycle, StartupPhase};
pub use session::ConnectionContext;
pub use shutdown::{ShutdownOutcome, ShutdownTrigger};
pub use startup::DaemonRuntime;

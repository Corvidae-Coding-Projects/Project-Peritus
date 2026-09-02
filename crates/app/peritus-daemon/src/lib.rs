//! Production daemon and application composition owner for Peritus.
//!
//! One serialized authority task owns writable state. IPC connections, subscriptions, providers,
//! tools, and background workers exchange bounded typed messages with that owner and never receive
//! a writable journal handle.

#![allow(
    dead_code,
    reason = "G0 retains closed typed worker, terminal, prompt, startup, and outbox seams that are exercised by focused tests before every producer is live in the application root"
)]
#![allow(
    clippy::double_must_use,
    clippy::large_enum_variant,
    clippy::large_types_passed_by_value,
    clippy::manual_let_else,
    clippy::match_same_arms,
    clippy::missing_errors_doc,
    clippy::missing_panics_doc,
    clippy::needless_pass_by_value,
    clippy::option_if_let_else,
    clippy::significant_drop_tightening,
    clippy::struct_field_names,
    clippy::suspicious_operation_groupings,
    clippy::too_many_lines,
    clippy::unnecessary_debug_formatting,
    clippy::unnecessary_wraps,
    clippy::unused_async,
    clippy::unused_self,
    reason = "G0 favors explicit identity-bearing state transitions and stable cross-platform signatures over style-only rewrites in the composition boundary"
)]

mod artifact;
mod authority;
mod cli;
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
mod product_run;
mod prompt;
pub(crate) mod qualification;
pub(crate) mod session;
mod shutdown;
mod startup;
mod subscription;
mod telemetry;
mod terminal;
pub mod verified;
mod worker;

pub use authority::{AuthorityHandle, AuthorityOwner};
pub use cli::run_cli;
pub use component::{
    DaemonComponents, DispatcherBinding, FilesystemDispatcherRoute, GitDispatcherRoute,
    OfficialExecutableSelection, ProviderAdapterKind, ProviderDeclaration, ProviderProfileKey,
    ProviderRegistry, ProviderRegistryError, ProviderRegistryErrorKind, ProviderRegistryLimits,
    ToolComponentError, ToolComponentErrorKind, ToolComponents, ToolDispatcherRoute,
    ToolRegistration,
};
pub use config::{
    ApprovalRegistryDeclaration, DaemonConfig, DaemonLimits, DaemonPaths, LocalHumanPrincipal,
    ProjectDeclaration, ProviderProfileDeclaration, ProviderRoute, ProviderRouteKind,
    TelemetryExport, ToolPolicy, WorkspaceDeclaration,
};
pub use error::{DaemonError, DaemonErrorCode, DaemonRecovery};
pub use identity::DaemonIdentity;
pub use ipc::{
    AppFrameStream, AuthenticatedConnection, LocalEndpoint, LocalEndpointAddress, PeerIdentity,
};
pub use lifecycle::{DaemonLifecycle, StartupPhase};
pub use prompt::PromptTerminalStatus;
pub use session::ConnectionContext;
pub use shutdown::{ShutdownOutcome, ShutdownTrigger};
pub use startup::DaemonRuntime;

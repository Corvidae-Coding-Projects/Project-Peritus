//! Explicit public facade; implementation modules retain their existing ownership.

pub use crate::authority::{AuthorityHandle, AuthorityOwner};
pub use crate::cli::run_cli;
pub use crate::component::{
    DaemonComponents, DispatcherBinding, FilesystemDispatcherRoute, GitDispatcherRoute,
    OfficialExecutableSelection, ProviderAdapterKind, ProviderDeclaration, ProviderProfileKey,
    ProviderRegistry, ProviderRegistryError, ProviderRegistryErrorKind, ProviderRegistryLimits,
    ToolComponentError, ToolComponentErrorKind, ToolComponents, ToolDispatcherRoute,
    ToolRegistration,
};
pub use crate::config::{
    ApprovalRegistryDeclaration, ContextPolicy, DaemonConfig, DaemonLimits, DaemonPaths,
    FolderDeclaration, LocalHumanPrincipal, ProjectDeclaration, ProviderProfileDeclaration,
    ProviderRoute, ProviderRouteKind, TelemetryExport, ToolPolicy, WorkspaceDeclaration,
};
pub use crate::error::{DaemonError, DaemonErrorCode, DaemonRecovery};
pub use crate::identity::DaemonIdentity;
pub use crate::ipc::{
    AppFrameStream, AuthenticatedConnection, LocalEndpoint, LocalEndpointAddress, PeerIdentity,
};
pub use crate::lifecycle::{DaemonLifecycle, StartupPhase};
pub use crate::prompt::PromptTerminalStatus;
pub use crate::session::ConnectionContext;
pub use crate::shutdown::{ShutdownOutcome, ShutdownTrigger};
pub use crate::startup::DaemonRuntime;

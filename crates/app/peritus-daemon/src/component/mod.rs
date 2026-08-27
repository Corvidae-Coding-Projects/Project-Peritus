//! Explicit application component inventories.
//!
//! Component registries are assembled once during startup. They contain only configured,
//! revision-bound capabilities and expose no runtime mutation surface.

mod credentials;
mod inventory;
mod profiles;
mod providers;
mod tools;

pub use credentials::PlatformCredentialSource;
pub use inventory::DaemonComponents;
pub use profiles::{OfficialExecutableSelection, ProviderDeclaration, ProviderProfileKey};
pub use providers::{
    ProviderAdapterKind, ProviderRegistry, ProviderRegistryError, ProviderRegistryErrorKind,
    ProviderRegistryLimits,
};
pub use tools::{
    DispatcherBinding, FilesystemDispatcherRoute, GitDispatcherRoute, ToolComponentError,
    ToolComponentErrorKind, ToolComponents, ToolDispatcherRoute, ToolRegistration,
};

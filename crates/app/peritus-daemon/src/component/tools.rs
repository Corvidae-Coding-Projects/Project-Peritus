//! Closed production C4 tool inventory and router composition.
//!
//! Startup selects only explicitly configured names from the compiled catalog. Runtime dispatchers
//! remain short-lived because the C4 adapters borrow exact C1/C2/C3 authority and target handles.

mod catalog;
mod error;
mod registry;
mod route;
mod selection;

pub use error::{ToolComponentError, ToolComponentErrorKind};
pub use registry::{DispatcherBinding, ToolComponents, ToolRegistration};
pub use route::{FilesystemDispatcherRoute, GitDispatcherRoute, ToolDispatcherRoute};

#[cfg(test)]
mod tests;

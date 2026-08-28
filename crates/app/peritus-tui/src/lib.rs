//! Interactive, protocol-faithful terminal client for Peritus.
//!
//! The crate owns presentation and local interaction state only. It sends checked A3 requests and
//! renders daemon observations; it never infers durable success or grants authority locally.

mod action;
mod client;
mod entry;
mod error;
mod input;
mod model;
mod render;
mod runtime;
mod sanitize;
mod terminal;

pub use entry::run_env;
pub use error::TuiError;
pub use runtime::{ExitReason, ProductLaunchContext, ProductProviderOption, TuiConfig, run};

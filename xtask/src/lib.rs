//! Packaging, installation, distribution, and release tools for Peritus.

pub(crate) mod cli;
pub(crate) mod distro;
pub(crate) mod error;
pub(crate) mod product_package;
pub(crate) mod release;

pub use cli::run_from_env;
pub use error::{Diagnostic, ErrorCode, XtaskError};

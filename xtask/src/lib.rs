#![doc = "Engineering policy checks for the Peritus workspace."]
#![allow(
    clippy::redundant_pub_crate,
    reason = "crate-scoped visibility documents xtask's internal module contract"
)]

pub(crate) mod architecture;
pub(crate) mod cli;
pub(crate) mod error;
pub(crate) mod metadata;
pub(crate) mod model;
pub(crate) mod reproducibility;
pub(crate) mod source;
pub(crate) mod toolchain;
pub(crate) mod trust;

pub use cli::run_from_env;
pub use error::{Diagnostic, ErrorCode, XtaskError};

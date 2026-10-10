//! Typed launch profiles, scoped capture receipts, and independently classified result evidence.

use crate::{AppErrorCode, AppProtocolError};

const MAX_PREVIEW_INPUT_BYTES: usize = 64 * 1024;

mod output;
pub use output::*;
mod profile;
pub use profile::*;
mod capture;
pub use capture::*;
mod result;
pub use result::*;

fn valid_environment_name(name: &str) -> bool {
    !name.is_empty() && !name.contains(['=', '\0'])
}

const fn invalid() -> AppProtocolError {
    AppProtocolError::new(AppErrorCode::MalformedFrame, None)
}

#[cfg(test)]
mod tests;

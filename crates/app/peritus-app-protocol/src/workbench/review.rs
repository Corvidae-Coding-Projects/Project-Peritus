//! Structured diff targets, anchored feedback, and qualification-evidence freshness.

use std::path::{Component, Path};

use crate::{AppErrorCode, AppProtocolError};

/// Maximum structured files in one bounded diff view.
pub const MAX_WORKBENCH_DIFF_FILES: usize = 512;
/// Maximum structured hunks in one bounded diff view.
pub const MAX_WORKBENCH_DIFF_HUNKS: usize = 4096;
/// Maximum visible lines in one bounded structured diff view.
pub const MAX_WORKBENCH_DIFF_LINES: usize = 32768;
/// Maximum review comments returned in one page.
pub const MAX_WORKBENCH_REVIEW_PAGE: usize = 256;
const MAX_PATH_BYTES: usize = 4096;

mod anchor;
pub use anchor::*;
mod diff;
pub use diff::*;
mod page;
pub use page::*;
mod parser;
pub use parser::parse_workbench_diff;

fn validate_path(value: &str) -> Result<(), AppProtocolError> {
    let path = Path::new(value);
    if value.is_empty()
        || value.len() > MAX_PATH_BYTES
        || path.is_absolute()
        || path.starts_with(".git")
        || path.components().any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(malformed());
    }
    Ok(())
}

const fn malformed() -> AppProtocolError {
    AppProtocolError::new(AppErrorCode::MalformedFrame, None)
}
const fn limit() -> AppProtocolError {
    AppProtocolError::new(AppErrorCode::LimitExceeded, None)
}

#[cfg(test)]
mod tests;

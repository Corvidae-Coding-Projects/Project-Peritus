//! Persistent native-controller adapter for production H1 subjects.

mod config;
mod controller;
mod diagnostics;
mod digest;
mod path;
mod process;
mod process_tree;
mod protocol;
mod subject;

pub use config::{NativeAdapterError, NativeControllerLimits};
pub use subject::NativeResilienceFactory;

use crate::{QualificationText, SubjectError, SubjectErrorCode};

fn subject_error(
    code: SubjectErrorCode,
    detail: impl Into<String>,
    retryable: bool,
) -> SubjectError {
    let detail = detail.into();
    let detail = bounded_detail(&detail);
    let context = QualificationText::from_sanitized(detail);
    SubjectError::new(code, context, retryable)
}

fn bounded_detail(value: &str) -> String {
    let mut output = String::with_capacity(value.len().min(1_024));
    for character in value.chars().filter(|character| !character.is_control()) {
        let next = output.len().saturating_add(character.len_utf8());
        if next > 1_024 {
            break;
        }
        output.push(character);
    }
    if output.trim().is_empty() { "native resilience adapter failed".to_owned() } else { output }
}

//! Current enabled-package authority and shared publication of native module controls.
use super::{ProductRunInput, ProductRunnerError, error};
use crate::execution::modules::artifacts::PublishedArtifact;
use peritus_modules::{Definition, Registry};
use peritus_plugin_sdk::ModuleMenuAction;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const SUMMARY_BYTES: usize = 16 * 1024;

pub(super) fn menu(
    input: &ProductRunInput,
    definition: &Definition,
    actions: &[ModuleMenuAction],
    text: &str,
    artifact: Option<&PublishedArtifact>,
    state: &str,
) -> Result<(), ProductRunnerError> {
    for action in actions {
        if action.label.trim().is_empty()
            || action.label.chars().any(char::is_control)
            || action.label.len() > 256
            || action.arguments.contains('\0')
        {
            return Err(error("native plugin returned an invalid menu action"));
        }
    }
    let registry = Registry::for_user().map_err(error)?;
    let current = registry.load(&definition.manifest.id, true).map_err(error)?;
    if current.digest != definition.digest {
        return Err(error("native plugin selection changed before result publication"));
    }
    let run = crate::encode_hex(input.run_id.as_bytes());
    let (summary, truncated) = summary(text);
    let progress = json!({
        "state":state,
        "summary":summary.lines().next().unwrap_or("Native module is waiting for input."),
    });
    registry
        .publish_session_view(
            &run,
            &definition.manifest.id,
            &definition.digest,
            &json!({
                "schema":1,
                "actions":actions,
                "summary":summary,
                "summary_bytes":text.len(),
                "summary_sha256":crate::encode_hex(&Sha256::digest(text.as_bytes())),
                "summary_truncated":truncated,
                "progress":progress,
                "artifact":artifact,
            }),
        )
        .map_err(error)
}

pub(super) fn progress(
    input: &ProductRunInput,
    definition: &Definition,
    progress: Value,
) -> Result<(), ProductRunnerError> {
    let run = crate::encode_hex(input.run_id.as_bytes());
    let progress = Box::new(progress);
    Registry::for_user()
        .map_err(error)?
        .publish_session_progress(&run, &definition.manifest.id, &definition.digest, &progress)
        .map_err(error)
}

fn summary(text: &str) -> (&str, bool) {
    if text.len() <= SUMMARY_BYTES {
        return (text, false);
    }
    let mut end = SUMMARY_BYTES;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[..end], true)
}

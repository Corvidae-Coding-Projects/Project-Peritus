//! Current enabled-package authority and shared publication of native module controls.
use super::{ProductRunInput, ProductRunnerError, error};
use peritus_modules::{Definition, Registry};
use peritus_plugin_sdk::ModuleMenuAction;
use serde_json::json;

pub(super) fn menu(
    input: &ProductRunInput,
    definition: &Definition,
    actions: &[ModuleMenuAction],
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
    let run = input.run_id.as_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>();
    registry
        .publish_session_view(
            &run,
            &definition.manifest.id,
            &definition.digest,
            &json!({"actions":actions}),
        )
        .map_err(error)
}

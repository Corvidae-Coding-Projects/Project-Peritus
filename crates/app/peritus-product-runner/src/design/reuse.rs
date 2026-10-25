//! Reuse a host-published task design without another discovery/provider pass.

use super::{DesignDocument, ProductRunInput, ProductRunnerError, publish};
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema: u32,
    workspace: [u8; 16],
    request: String,
    design: String,
    #[serde(default)]
    guided: Option<String>,
    #[serde(default)]
    revision: u64,
}

fn digest(bytes: &[u8]) -> String {
    format!("{:02x?}", peritus_codec::sha256(bytes).as_bytes())
}

fn request(input: &ProductRunInput) -> String {
    digest(format!("{}\0{}", input.workspace_root.display(), input.task).as_bytes())
}

pub(super) fn load(input: &ProductRunInput) -> Result<Option<DesignDocument>, ProductRunnerError> {
    let path = input.trace_path.with_extension("design.md");
    if let Ok(bytes) = fs::read(input.trace_path.with_extension("design-receipt.json"))
        && let Ok(saved) = serde_json::from_slice::<Receipt>(&bytes)
        && matches!(saved.schema, 1 | 2)
        && saved.workspace == *input.workspace_id.as_bytes()
        && (saved.schema == 1 || saved.request == request(input))
        && saved.guided == guided_digest(input)
        && let Ok(markdown) = fs::read_to_string(&path)
        && saved.design == digest(markdown.as_bytes())
    {
        crate::efficiency::event(
            &input.trace_path,
            serde_json::json!({"event":"design-reused","source":"host-receipt"}),
        );
        // A run-scoped v1 receipt predates immutable task identity. Recover its plan as prior
        // evidence, never as approval of new input; current governed instructions reach the writer.
        let revision = if saved.revision == 0 {
            input.conversation.incorporated_revision()
        } else {
            saved.revision
        };
        return Ok(Some(DesignDocument::restored(path, markdown, revision)));
    }
    // Entering implementation is already an explicit host/user operation. Reuse the ready
    // interview result as its contract; the writer resolves only current target details.
    if let Some(markdown) = crate::guided_design::ready_implementation(input)? {
        crate::efficiency::event(
            &input.trace_path,
            serde_json::json!({"event":"design-reused","source":"ready-guided-design"}),
        );
        let markdown = format!(
            "{markdown}\n\n## Implementation handoff\n\nThis ready design is the accepted task brief. Follow current literal user instructions if they amend it. Reuse host-verified source observations; refresh changed targets and resolve only the concrete technical questions needed for the next slice. No repeat whole-repository discovery is required.\n"
        );
        publish(&path, markdown.as_bytes())?;
        let document = DesignDocument::restored(path, markdown, input.conversation.revision());
        if let Err(error) = save(input, &document) {
            eprintln!("peritus: design reuse receipt unavailable: {error}");
        }
        return Ok(Some(document));
    }
    Ok(None)
}

pub(super) fn save(
    input: &ProductRunInput,
    design: &DesignDocument,
) -> Result<(), ProductRunnerError> {
    let receipt = Receipt {
        schema: 2,
        workspace: *input.workspace_id.as_bytes(),
        request: request(input),
        design: digest(design.markdown().as_bytes()),
        guided: guided_digest(input),
        revision: design.conversation_revision(),
    };
    let bytes =
        serde_json::to_vec(&receipt).map_err(|error| super::filesystem(error.to_string()))?;
    publish(&input.trace_path.with_extension("design-receipt.json"), &bytes)
}

fn guided_digest(input: &ProductRunInput) -> Option<String> {
    fs::read(crate::guided_design::path(input)).ok().map(|bytes| digest(&bytes))
}

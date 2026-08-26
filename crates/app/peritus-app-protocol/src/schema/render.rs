//! Deterministic JSON Schema, TypeScript, and registry renderers.

mod json;
mod registry_doc;
mod typescript;

/// One generated UTF-8 artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GeneratedTextArtifact {
    /// Repository-relative output path.
    pub path: &'static str,
    /// Complete deterministic contents.
    pub content: String,
}

/// Renders every checked-in application-protocol text artifact.
#[must_use]
pub fn generated_text_artifacts() -> Vec<GeneratedTextArtifact> {
    vec![
        GeneratedTextArtifact {
            path: "app-protocol/generated/peritus-app-v1.schema.json",
            content: json::render(),
        },
        GeneratedTextArtifact {
            path: "app-protocol/generated/peritus-app-v1.ts",
            content: typescript::render(),
        },
        GeneratedTextArtifact {
            path: "app-protocol/generated/peritus-app-v1.registry.md",
            content: registry_doc::render(),
        },
    ]
}

//! Human-readable application registry rendering.

use super::super::{APP_ERROR_CODES, APP_FAMILIES, APP_FLOW_TYPES, APP_NESTED_TYPES};

pub(super) fn render() -> String {
    let mut output = String::from(
        "# Peritus application protocol v1 registry\n\nGenerated from Rust metadata. Numeric and semantic allocations are append-only.\n\n## Families\n\n| Tag | Family | Schema | Payloads |\n|---:|---|---:|---|\n",
    );
    for family in APP_FAMILIES {
        output.push_str("| ");
        output.push_str(&family.tag.to_string());
        output.push_str(" | `");
        output.push_str(family.name);
        output.push_str("` | ");
        output.push_str(&family.schema_version.to_string());
        output.push_str(" | ");
        for (index, payload) in family.payloads.iter().enumerate() {
            if index > 0 {
                output.push_str(", ");
            }
            output.push('`');
            output.push_str(&payload.tag.to_string());
            output.push(':');
            output.push_str(payload.name);
            output.push('`');
        }
        output.push_str(" |\n");
    }
    output.push_str("\n## Typed fields\n");
    for descriptor in APP_NESTED_TYPES.iter().chain(APP_FLOW_TYPES).flat_map(|group| group.iter()) {
        output.push_str("\n### `");
        output.push_str(descriptor.name);
        output.push_str("`\n\nRust type: `");
        output.push_str(descriptor.rust_type);
        output.push_str("`\n\n| Field | Required | Canonical wire | Rust | TypeScript | Bounds |\n|---|:---:|---|---|---|---|\n");
        for field in descriptor.fields {
            output.push_str("| `");
            output.push_str(field.name);
            output.push_str("` | ");
            output.push_str(if field.required { "yes" } else { "no" });
            output.push_str(" | `");
            output.push_str(field.wire_type.as_str());
            output.push_str("` | `");
            output.push_str(field.rust_type);
            output.push_str("` | `");
            output.push_str(field.typescript_type);
            output.push_str("` | ");
            if field.bounds.is_empty() {
                output.push('—');
            } else {
                for (index, bound) in field.bounds.iter().enumerate() {
                    if index > 0 {
                        output.push_str(", ");
                    }
                    output.push('`');
                    output.push_str(bound.as_str());
                    output.push('`');
                }
            }
            output.push_str(" |\n");
        }
    }
    output.push_str("\n## Stable errors\n\n| Tag | Code |\n|---:|---|\n");
    for error in APP_ERROR_CODES {
        output.push_str("| ");
        output.push_str(&error.tag.to_string());
        output.push_str(" | `");
        output.push_str(error.name);
        output.push_str("` |\n");
    }
    output
}

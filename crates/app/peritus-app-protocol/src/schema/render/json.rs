//! Draft 2020-12 JSON Schema rendering.

use super::super::{
    APP_ERROR_CODES, APP_FAMILIES, APP_FLOW_TYPES, APP_NESTED_TYPES, AppFieldDescriptor,
    AppTypeDescriptor, JsonShape,
};

pub(super) fn render() -> String {
    let mut output = String::from(
        "{\n  \"$schema\": \"https://json-schema.org/draft/2020-12/schema\",\n  \"$id\": \"https://peritus.invalid/schema/app/v1\",\n  \"title\": \"Peritus application protocol v1\",\n  \"type\": \"object\",\n  \"additionalProperties\": false,\n  \"required\": [\"family\", \"schemaVersion\", \"payloadKind\", \"payload\"],\n  \"properties\": {\n    \"family\": { \"enum\": [",
    );
    separated(&mut output, APP_FAMILIES.iter().map(|family| family.tag.to_string()));
    output.push_str(
        "] },\n    \"schemaVersion\": { \"const\": 1 },\n    \"payloadKind\": { \"enum\": [",
    );
    separated(
        &mut output,
        APP_FAMILIES
            .iter()
            .flat_map(|family| family.payloads.iter())
            .map(|payload| quoted(payload.name)),
    );
    output.push_str(
        "] },\n    \"payload\": { \"type\": \"string\", \"contentEncoding\": \"base64\", \"description\": \"Exact canonical semantic payload bytes\" }\n  },\n  \"$defs\": {\n",
    );
    render_error_codes(&mut output);
    for descriptor in APP_NESTED_TYPES.iter().chain(APP_FLOW_TYPES).flat_map(|group| group.iter()) {
        output.push_str(",\n");
        render_type(&mut output, descriptor);
    }
    output.push_str("\n  }\n}\n");
    output
}

fn render_error_codes(output: &mut String) {
    output.push_str("    \"AppErrorCode\": { \"type\": \"string\", \"enum\": [");
    separated(output, APP_ERROR_CODES.iter().map(|error| quoted(error.name)));
    output.push_str("] }");
}

fn render_type(output: &mut String, descriptor: &AppTypeDescriptor) {
    output.push_str("    \"");
    output.push_str(descriptor.name);
    output.push_str("\": {\n      \"type\": \"object\",\n      \"additionalProperties\": false,\n      \"x-rust-type\": ");
    output.push_str(&quoted(descriptor.rust_type));
    output.push_str(",\n      \"properties\": {\n");
    for (index, field) in descriptor.fields.iter().enumerate() {
        output.push_str("        \"");
        output.push_str(field.name);
        output.push_str("\": ");
        render_field(output, field);
        output.push_str(if index + 1 == descriptor.fields.len() { "\n" } else { ",\n" });
    }
    output.push_str("      }");
    let required = descriptor.fields.iter().filter(|field| field.required).collect::<Vec<_>>();
    if !required.is_empty() {
        output.push_str(",\n      \"required\": [");
        separated(output, required.into_iter().map(|field| quoted(field.name)));
        output.push(']');
    }
    output.push_str("\n    }");
}

fn render_field(output: &mut String, field: &AppFieldDescriptor) {
    output.push_str("{ ");
    render_shape(output, field.json_shape);
    output.push_str(", \"x-canonical-wire\": ");
    output.push_str(&quoted(field.wire_type.as_str()));
    output.push_str(", \"x-rust-type\": ");
    output.push_str(&quoted(field.rust_type));
    if !field.bounds.is_empty() {
        output.push_str(", \"x-bounds\": [");
        separated(output, field.bounds.iter().map(|bound| quoted(bound.as_str())));
        output.push(']');
    }
    output.push_str(" }");
}

fn render_shape(output: &mut String, shape: JsonShape) {
    match shape {
        JsonShape::Boolean => output.push_str("\"type\": \"boolean\""),
        JsonShape::U16 => {
            output.push_str("\"type\": \"integer\", \"minimum\": 0, \"maximum\": 65535");
        }
        JsonShape::U32 => {
            output.push_str("\"type\": \"integer\", \"minimum\": 0, \"maximum\": 4294967295");
        }
        JsonShape::I32 => output
            .push_str("\"type\": \"integer\", \"minimum\": -2147483648, \"maximum\": 2147483647"),
        JsonShape::U64String => {
            output.push_str("\"type\": \"string\", \"pattern\": \"^(0|[1-9][0-9]{0,19})$\"");
        }
        JsonShape::String => output.push_str("\"type\": \"string\""),
        JsonShape::Base64 => {
            output.push_str("\"type\": \"string\", \"contentEncoding\": \"base64\"");
        }
        JsonShape::Identifier => {
            output.push_str("\"type\": \"string\", \"pattern\": \"^[0-9a-f]{32}$\"");
        }
        JsonShape::Digest => {
            output.push_str("\"type\": \"string\", \"pattern\": \"^[0-9a-f]{64}$\"");
        }
        JsonShape::Enum(values) => {
            output.push_str("\"type\": \"string\", \"enum\": [");
            separated(output, values.iter().map(|value| quoted(value)));
            output.push(']');
        }
        JsonShape::Ref(name) => {
            output.push_str("\"$ref\": ");
            output.push_str(&quoted(&format!("#/$defs/{name}")));
        }
        JsonShape::ArrayRef(name) => {
            output.push_str("\"type\": \"array\", \"items\": { \"$ref\": ");
            output.push_str(&quoted(&format!("#/$defs/{name}")));
            output.push_str(" }");
        }
        JsonShape::StringArray => {
            output.push_str("\"type\": \"array\", \"items\": { \"type\": \"string\" }");
        }
    }
}

fn quoted(value: &str) -> String {
    format!("\"{value}\"")
}

fn separated(output: &mut String, values: impl Iterator<Item = String>) {
    for (index, value) in values.enumerate() {
        if index > 0 {
            output.push_str(", ");
        }
        output.push_str(&value);
    }
}

//! Workspace admission uses the same bounded argument contracts as the common agent boundary.
use super::{catalog, path::tool};
use peritus_agent::DeveloperLoopError;
use peritus_model_protocol::{
    JsonBounds, JsonSchema, ProtocolLimits, SchemaDialect, ToolArgumentSchema,
};
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};
type Catalog = BTreeMap<String, (Value, ToolArgumentSchema)>;
static CATALOG: OnceLock<Result<Catalog, String>> = OnceLock::new();
pub(super) fn validate(name: &str, arguments: &Value) -> Result<(), DeveloperLoopError> {
    // Backend configuration can change between invocations in the same daemon.
    // The outer loop validates its advertised snapshot; this guard also checks current capabilities.
    if name == "web_search" {
        let definition = super::web::definitions()
            .map_err(|e| tool(e.to_string()))?
            .into_iter()
            .find(|d| d.name().as_str() == name)
            .ok_or_else(|| tool("missing web search contract"))?;
        return ToolArgumentSchema::compile(definition.parameters())
            .and_then(|s| s.validate_value(arguments))
            .map_err(|e| tool(format!("Invalid tool arguments: {e}. No action was executed.")));
    }
    let catalog = CATALOG
        .get_or_init(load)
        .as_ref()
        .map_err(|e| tool(format!("invalid host tool catalog: {e}")))?;
    let (_, schema) =
        catalog.get(name).ok_or_else(|| tool("model requested an undeclared developer tool"))?;
    schema.validate_value(arguments).map_err(|e|tool(format!("Invalid tool arguments: {e}. No action was executed. Correct only the rejected fields and continue the current task; do not interpret this rejection as a missing file or service.")))
}
pub(super) fn validate_deferred(
    name: &str,
    arguments: &Value,
    deferred: &[String],
) -> Result<(), DeveloperLoopError> {
    let catalog = CATALOG
        .get_or_init(load)
        .as_ref()
        .map_err(|e| tool(format!("invalid host tool catalog: {e}")))?;
    let (original, _) = catalog.get(name).ok_or_else(|| tool("undeclared inspection tool"))?;
    let mut schema = original.clone();
    for field in deferred {
        if schema["properties"].get(field).is_none() {
            return Err(tool("plan result reference names an undeclared argument"));
        }
    }
    if let Some(required) = schema.get_mut("required").and_then(Value::as_array_mut) {
        required.retain(|field| !deferred.iter().any(|name| field.as_str() == Some(name)));
    }
    compile(&schema)
        .and_then(|s| s.validate_value(arguments).map_err(|e| e.to_string()))
        .map_err(tool)
}
fn compile(value: &Value) -> Result<ToolArgumentSchema, String> {
    let schema = JsonSchema::parse(
        &value.to_string(),
        SchemaDialect::Draft202012,
        JsonBounds::schema(ProtocolLimits::PRODUCTION),
    )
    .map_err(|e| e.to_string())?;
    ToolArgumentSchema::compile(&schema).map_err(|e| e.to_string())
}
fn load() -> Result<Catalog, String> {
    let mut definitions = catalog::definitions().map_err(|e| e.to_string())?;
    definitions.push(catalog::in_place_definition().map_err(|e| e.to_string())?);
    definitions.extend(catalog::reviewer_definitions().map_err(|e| e.to_string())?);
    definitions
        .into_iter()
        .map(|definition| {
            let schema: Value = serde_json::from_slice(definition.parameters().canonical_bytes())
                .map_err(|e| e.to_string())?;
            let compiled = compile(&schema)?;
            Ok((definition.name().as_str().to_owned(), (schema, compiled)))
        })
        .collect()
}

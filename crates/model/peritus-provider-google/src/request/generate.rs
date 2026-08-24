//! Stable-v1 Generate Content contents, functions, cache, and generation projection.

use peritus_model_protocol::{
    CachePolicy, ContentBlock, ModelRequest, ReasoningEffort, ReasoningPolicy, Role,
    StructuredOutput, SummaryPolicy, ToolChoice,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::{Map, Value};

use super::content::{generate_part, generate_tool};
use super::invalid;
use super::value::{insert, millionths, object, parse, string};

pub(super) fn project(request: &ModelRequest) -> Result<Value, ProviderCoreError> {
    if request.options().continuation().is_some() || request.options().persistence().store() {
        return Err(invalid("Generate Content is stateless and has no response continuation"));
    }
    let (system, contents) = contents(request)?;
    if contents.is_empty() {
        return Err(invalid("Generate Content requires at least one conversational content"));
    }
    let mut value = Map::new();
    value.insert("contents".to_owned(), Value::Array(contents));
    insert(&mut value, "systemInstruction", system);
    if !request.tools().is_empty() {
        let declarations =
            request.tools().iter().map(generate_tool).collect::<Result<Vec<_>, _>>()?;
        value.insert(
            "tools".to_owned(),
            Value::Array(vec![object([("functionDeclarations", Value::Array(declarations))])]),
        );
        value.insert("toolConfig".to_owned(), tool_config(request.tool_choice()));
    }
    value.insert("generationConfig".to_owned(), generation(request)?);
    match request.options().cache() {
        CachePolicy::Disabled | CachePolicy::Automatic => {}
        CachePolicy::Explicit(key) => {
            value.insert("cachedContent".to_owned(), string(key.expose_for_wire()));
        }
        CachePolicy::Ephemeral { .. } => {
            return Err(invalid(
                "Generate Content can reuse cachedContent but cannot create cache resources",
            ));
        }
    }
    Ok(Value::Object(value))
}

fn contents(request: &ModelRequest) -> Result<(Option<Value>, Vec<Value>), ProviderCoreError> {
    let mut system_parts = Vec::new();
    let mut contents = Vec::new();
    for message in request.messages() {
        match message.role() {
            Role::System | Role::Developer => {
                for block in message.content() {
                    let ContentBlock::Text(text) = block else {
                        return Err(invalid("Google system instruction accepts text only"));
                    };
                    system_parts.push(object([("text", string(text.expose_for_wire()))]));
                }
            }
            role => {
                let parts =
                    message.content().iter().map(generate_part).collect::<Result<Vec<_>, _>>()?;
                contents.push(object([
                    ("role", string(role_name(role))),
                    ("parts", Value::Array(parts)),
                ]));
            }
        }
    }
    let system =
        (!system_parts.is_empty()).then(|| object([("parts", Value::Array(system_parts))]));
    Ok((system, contents))
}

const fn role_name(role: Role) -> &'static str {
    match role {
        Role::Assistant => "model",
        Role::User | Role::Tool | Role::System | Role::Developer => "user",
    }
}

fn tool_config(choice: &ToolChoice) -> Value {
    let (mode, names) = match choice {
        ToolChoice::Auto => ("AUTO", None),
        ToolChoice::None => ("NONE", None),
        ToolChoice::Required => ("ANY", None),
        ToolChoice::Specific(name) => ("ANY", Some(vec![string(name.as_str())])),
    };
    let mut calling = Map::new();
    calling.insert("mode".to_owned(), string(mode));
    insert(&mut calling, "allowedFunctionNames", names.map(Value::Array));
    object([("functionCallingConfig", Value::Object(calling))])
}

fn generation(request: &ModelRequest) -> Result<Value, ProviderCoreError> {
    let controls = request.options().generation();
    let mut value = Map::new();
    value.insert("maxOutputTokens".to_owned(), Value::from(controls.max_output_tokens()));
    insert(
        &mut value,
        "stopSequences",
        (!controls.stop_sequences().is_empty()).then(|| {
            Value::Array(
                controls
                    .stop_sequences()
                    .iter()
                    .map(|item| string(item.expose_for_wire()))
                    .collect(),
            )
        }),
    );
    insert(&mut value, "seed", controls.seed().map(Value::from));
    insert(&mut value, "temperature", controls.temperature_millionths().map(millionths));
    insert(&mut value, "topP", controls.top_p_millionths().map(millionths));
    output(request.options().output(), &mut value)?;
    insert(&mut value, "thinkingConfig", thinking(request.options().reasoning())?);
    Ok(Value::Object(value))
}

fn output(
    output: &StructuredOutput,
    value: &mut Map<String, Value>,
) -> Result<(), ProviderCoreError> {
    match output {
        StructuredOutput::Text => Ok(()),
        StructuredOutput::JsonObject => {
            value.insert("responseMimeType".to_owned(), string("application/json"));
            value.insert("responseJsonSchema".to_owned(), object([("type", string("object"))]));
            Ok(())
        }
        StructuredOutput::JsonSchema { schema, strict, .. } => {
            if !strict || schema.dialect() != peritus_model_protocol::SchemaDialect::GeminiSubset {
                return Err(invalid(
                    "Generate Content response schema requires strict Gemini-subset JSON Schema",
                ));
            }
            value.insert("responseMimeType".to_owned(), string("application/json"));
            value.insert("responseJsonSchema".to_owned(), parse(schema.canonical_bytes())?);
            Ok(())
        }
    }
}

fn thinking(policy: ReasoningPolicy) -> Result<Option<Value>, ProviderCoreError> {
    let (level, include_parts) = match policy {
        ReasoningPolicy::Disabled => return Ok(None),
        ReasoningPolicy::Adaptive { summary } => (None, include_thoughts(summary)?),
        ReasoningPolicy::Effort { effort, summary } => {
            let level = match effort {
                ReasoningEffort::Minimal => "minimal",
                ReasoningEffort::Low => "low",
                ReasoningEffort::Medium => "medium",
                ReasoningEffort::High => "high",
            };
            (Some(level), include_thoughts(summary)?)
        }
    };
    let mut value = Map::new();
    insert(&mut value, "thinkingLevel", level.map(string));
    value.insert("includeThoughts".to_owned(), Value::Bool(include_parts));
    Ok(Some(Value::Object(value)))
}

const fn include_thoughts(summary: SummaryPolicy) -> Result<bool, ProviderCoreError> {
    match summary {
        SummaryPolicy::None => Ok(false),
        SummaryPolicy::Auto => Ok(true),
        SummaryPolicy::Concise | SummaryPolicy::Detailed => Err(invalid(
            "Google thinking supports included or omitted thoughts, not summary length",
        )),
    }
}

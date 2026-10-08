//! Stable-v1 Generate Content contents, functions, cache, and generation projection.

use std::collections::BTreeMap;

use peritus_model_protocol::{
    CachePolicy, ContentBlock, ModelRequest, ReasoningEffort, ReasoningPolicy, Role,
    StructuredOutput, SummaryPolicy, ToolCallId, ToolChoice, ToolName,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::{Map, Value};

use super::content::{generate_part, generate_tool};
use super::invalid;
use super::value::{insert, millionths, object, parse, string};

pub(super) struct Projection {
    message: usize,
    block: usize,
    system_parts: Vec<Value>,
    contents: Vec<Value>,
    current_parts: Vec<Value>,
    preceding_calls: BTreeMap<ToolCallId, ToolName>,
}

impl Projection {
    pub(super) const fn new() -> Self {
        Self {
            message: 0,
            block: 0,
            system_parts: Vec::new(),
            contents: Vec::new(),
            current_parts: Vec::new(),
            preceding_calls: BTreeMap::new(),
        }
    }

    pub(super) fn advance(
        &mut self,
        request: &ModelRequest,
    ) -> Result<bool, ProviderCoreError> {
        let Some(message) = request.messages().get(self.message) else { return Ok(true) };
        let Some(block) = message.content().get(self.block) else {
            if !matches!(message.role(), Role::System | Role::Developer) {
                self.contents.push(object([
                    ("role", string(role_name(message.role()))),
                    ("parts", Value::Array(core::mem::take(&mut self.current_parts))),
                ]));
            }
            self.message = self
                .message
                .checked_add(1)
                .ok_or_else(|| invalid("Generate Content message cursor overflowed"))?;
            self.block = 0;
            return Ok(self.message == request.messages().len());
        };
        match message.role() {
            Role::System | Role::Developer => {
                let ContentBlock::Text(text) = block else {
                    return Err(invalid("Google system instruction accepts text only"));
                };
                self.system_parts.push(object([("text", string(text.expose_for_wire()))]));
            }
            _ => {
                if let ContentBlock::ToolCall(call) = block
                    && self
                        .preceding_calls
                        .insert(call.id().clone(), call.name().clone())
                        .is_some()
                {
                    return Err(invalid("Google history reused a function-call identity"));
                }
                self.current_parts.extend(generate_part(block, &self.preceding_calls)?);
            }
        }
        self.block = self
            .block
            .checked_add(1)
            .ok_or_else(|| invalid("Generate Content block cursor overflowed"))?;
        Ok(false)
    }

    pub(super) fn finish(self, request: &ModelRequest) -> Result<Value, ProviderCoreError> {
        if self.contents.is_empty() {
            return Err(invalid("Generate Content requires at least one conversational content"));
        }
        let system = (!self.system_parts.is_empty())
            .then(|| object([("parts", Value::Array(self.system_parts))]));
        project_fields(request, system, self.contents)
    }
}

fn project_fields(
    request: &ModelRequest,
    system: Option<Value>,
    contents: Vec<Value>,
) -> Result<Value, ProviderCoreError> {
    if request.options().continuation().is_some() || request.options().persistence().store() {
        return Err(invalid("Generate Content is stateless and has no response continuation"));
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
                ReasoningEffort::XHigh | ReasoningEffort::Max | ReasoningEffort::Ultra => {
                    return Err(invalid(
                        "Google does not map xhigh, max, or ultra reasoning effort",
                    ));
                }
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

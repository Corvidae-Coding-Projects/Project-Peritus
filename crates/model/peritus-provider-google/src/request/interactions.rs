//! Stable-v1 Interactions input, steps, tools, state, and generation projection.

use peritus_model_protocol::{
    CachePolicy, ContentBlock, ModelRequest, ReasoningEffort, ReasoningPolicy, Role,
    StructuredOutput, SummaryPolicy, ToolChoice,
};
use peritus_provider_core::ProviderCoreError;
use serde_json::{Map, Value};

use super::content::{interaction_content, interaction_replay, interaction_tool};
use super::invalid;
use super::value::{insert, millionths, object, parse, string};

pub(super) struct Projection {
    message: usize,
    block: usize,
    system: Vec<String>,
    steps: Vec<Value>,
    current_user: Vec<Value>,
}

impl Projection {
    pub(super) const fn new() -> Self {
        Self {
            message: 0,
            block: 0,
            system: Vec::new(),
            steps: Vec::new(),
            current_user: Vec::new(),
        }
    }

    pub(super) fn advance(
        &mut self,
        request: &ModelRequest,
    ) -> Result<bool, ProviderCoreError> {
        let Some(message) = request.messages().get(self.message) else { return Ok(true) };
        let Some(block) = message.content().get(self.block) else {
            if message.role() == Role::User {
                self.steps.push(object([
                    ("type", string("user_input")),
                    ("content", Value::Array(core::mem::take(&mut self.current_user))),
                ]));
            }
            self.message = self
                .message
                .checked_add(1)
                .ok_or_else(|| invalid("Google Interactions message cursor overflowed"))?;
            self.block = 0;
            return Ok(self.message == request.messages().len());
        };
        match message.role() {
            Role::System | Role::Developer => {
                let ContentBlock::Text(text) = block else {
                    return Err(invalid("Google system instruction accepts text only"));
                };
                self.system.push(text.expose_for_wire().to_owned());
            }
            Role::User => self.current_user.push(interaction_content(block)?),
            Role::Assistant => self.steps.push(assistant_step(block)?),
            Role::Tool => self.steps.push(tool_step(block)?),
        }
        self.block = self
            .block
            .checked_add(1)
            .ok_or_else(|| invalid("Google Interactions block cursor overflowed"))?;
        Ok(false)
    }

    pub(super) fn finish(self, request: &ModelRequest) -> Result<Value, ProviderCoreError> {
        let system = (!self.system.is_empty()).then(|| self.system.join("\n"));
        project_fields(request, system, self.steps)
    }
}

fn project_fields(
    request: &ModelRequest,
    system: Option<String>,
    input: Vec<Value>,
) -> Result<Value, ProviderCoreError> {
    let continuation = request.options().continuation();
    if let Some(continuation) = continuation
        && (!request.options().persistence().store()
            || continuation.event_id().is_some()
            || continuation.sequence().is_some())
    {
        return Err(invalid(
            "Google Interactions continuation requires storage and permits no exact cursor",
        ));
    }
    if input.is_empty() && continuation.is_none() {
        return Err(invalid("Google Interactions requires non-system input or continuation"));
    }
    let mut value = Map::new();
    value.insert("model".to_owned(), string(request.model().as_str()));
    value.insert("input".to_owned(), Value::Array(input));
    value.insert("stream".to_owned(), Value::Bool(true));
    value.insert("store".to_owned(), Value::Bool(request.options().persistence().store()));
    value.insert("background".to_owned(), Value::Bool(false));
    insert(&mut value, "system_instruction", system.map(Value::String));
    let tools = request.tools().iter().map(interaction_tool).collect::<Result<Vec<_>, _>>()?;
    insert(&mut value, "tools", (!tools.is_empty()).then_some(Value::Array(tools)));
    insert(&mut value, "response_format", response_format(request.options().output())?);
    value.insert("generation_config".to_owned(), generation(request)?);
    if let Some(continuation) = continuation {
        value.insert(
            "previous_interaction_id".to_owned(),
            string(continuation.response_id().expose_for_wire()),
        );
    }
    match request.options().cache() {
        CachePolicy::Disabled | CachePolicy::Automatic => {}
        CachePolicy::Explicit(_) | CachePolicy::Ephemeral { .. } => {
            return Err(invalid(
                "Google Interactions exposes implicit/state caching, not cachedContent resources",
            ));
        }
    }
    Ok(Value::Object(value))
}

fn assistant_step(block: &ContentBlock) -> Result<Value, ProviderCoreError> {
    match block {
        ContentBlock::Text(_) | ContentBlock::Refusal(_) => Ok(object([
            ("type", string("model_output")),
            ("content", Value::Array(vec![interaction_content(block)?])),
        ])),
        ContentBlock::ToolCall(call) => Ok(object([
                ("type", string("function_call")),
                ("id", string(call.id().expose_for_wire())),
                ("name", string(call.name().as_str())),
                ("arguments", parse(call.arguments().canonical_bytes())?),
        ])),
        ContentBlock::Reasoning(replay) => interaction_replay(replay),
        _ => Err(invalid("assistant block is unsupported by Google Interactions")),
    }
}

fn tool_step(block: &ContentBlock) -> Result<Value, ProviderCoreError> {
    let ContentBlock::ToolResult(result) = block else {
        return Err(invalid("Google tool messages require function results"));
    };
    Ok(object([
        ("type", string("function_result")),
        ("call_id", string(result.call_id().expose_for_wire())),
        ("result", parse(result.output().canonical_bytes())?),
        ("is_error", Value::Bool(result.is_error())),
    ]))
}

fn tool_choice(choice: &ToolChoice) -> Value {
    match choice {
        ToolChoice::Auto => string("auto"),
        ToolChoice::None => string("none"),
        ToolChoice::Required => string("any"),
        ToolChoice::Specific(name) => object([(
            "allowed_tools",
            object([("mode", string("any")), ("tools", Value::Array(vec![string(name.as_str())]))]),
        )]),
    }
}

fn response_format(output: &StructuredOutput) -> Result<Option<Value>, ProviderCoreError> {
    match output {
        StructuredOutput::Text => Ok(None),
        StructuredOutput::JsonObject => Ok(Some(object([
            ("type", string("text")),
            ("mime_type", string("application/json")),
            ("schema", object([("type", string("object"))])),
        ]))),
        StructuredOutput::JsonSchema { schema, strict, .. } => {
            if !strict || schema.dialect() != peritus_model_protocol::SchemaDialect::GeminiSubset {
                return Err(invalid(
                    "Google response format requires strict Gemini-subset JSON Schema",
                ));
            }
            Ok(Some(object([
                ("type", string("text")),
                ("mime_type", string("application/json")),
                ("schema", parse(schema.canonical_bytes())?),
            ])))
        }
    }
}

fn generation(request: &ModelRequest) -> Result<Value, ProviderCoreError> {
    let options = request.options();
    let controls = options.generation();
    let mut value = Map::new();
    value.insert("max_output_tokens".to_owned(), Value::from(controls.max_output_tokens()));
    insert(
        &mut value,
        "stop_sequences",
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
    insert(&mut value, "top_p", controls.top_p_millionths().map(millionths));
    let (level, summaries) = thinking(options.reasoning())?;
    insert(&mut value, "thinking_level", level.map(string));
    insert(&mut value, "thinking_summaries", summaries.map(string));
    value.insert("tool_choice".to_owned(), tool_choice(request.tool_choice()));
    Ok(Value::Object(value))
}

fn thinking(
    policy: ReasoningPolicy,
) -> Result<(Option<&'static str>, Option<&'static str>), ProviderCoreError> {
    match policy {
        ReasoningPolicy::Disabled => Ok((None, None)),
        ReasoningPolicy::Adaptive { summary } => Ok((None, summary_value(summary)?)),
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
            Ok((Some(level), summary_value(summary)?))
        }
    }
}

const fn summary_value(summary: SummaryPolicy) -> Result<Option<&'static str>, ProviderCoreError> {
    match summary {
        SummaryPolicy::None => Ok(Some("none")),
        SummaryPolicy::Auto => Ok(Some("auto")),
        SummaryPolicy::Concise | SummaryPolicy::Detailed => Err(invalid(
            "Google thinking supports auto or no summary, not requested summary length",
        )),
    }
}

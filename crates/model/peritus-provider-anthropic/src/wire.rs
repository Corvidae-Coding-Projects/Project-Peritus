//! Private Anthropic Messages request wire ownership and explicit JSON projection.

use serde_json::{Map, Value};

pub struct WireRequest {
    pub model: String,
    pub max_tokens: u64,
    pub messages: Vec<WireMessage>,
    pub stream: bool,
    pub system: Option<Vec<Value>>,
    pub tools: Option<Vec<Value>>,
    pub tool_choice: Option<Value>,
    pub stop_sequences: Option<Vec<String>>,
    pub temperature: Option<f64>,
    pub top_p: Option<f64>,
    pub thinking: Option<Value>,
    pub output_config: Option<Value>,
}

pub struct WireMessage {
    pub role: &'static str,
    pub content: Vec<Value>,
}

impl WireRequest {
    pub fn into_value(self) -> Value {
        let mut value = Map::new();
        value.insert("model".to_owned(), Value::String(self.model));
        value.insert("max_tokens".to_owned(), Value::from(self.max_tokens));
        value.insert(
            "messages".to_owned(),
            Value::Array(self.messages.into_iter().map(WireMessage::into_value).collect()),
        );
        value.insert("stream".to_owned(), Value::Bool(self.stream));
        insert_optional(&mut value, "system", self.system.map(Value::Array));
        insert_optional(&mut value, "tools", self.tools.map(Value::Array));
        insert_optional(&mut value, "tool_choice", self.tool_choice);
        insert_optional(
            &mut value,
            "stop_sequences",
            self.stop_sequences
                .map(|items| Value::Array(items.into_iter().map(Value::String).collect())),
        );
        insert_optional(&mut value, "temperature", self.temperature.map(Value::from));
        insert_optional(&mut value, "top_p", self.top_p.map(Value::from));
        insert_optional(&mut value, "thinking", self.thinking);
        insert_optional(&mut value, "output_config", self.output_config);
        Value::Object(value)
    }
}

impl WireMessage {
    fn into_value(self) -> Value {
        let mut value = Map::new();
        value.insert("role".to_owned(), Value::String(self.role.to_owned()));
        value.insert("content".to_owned(), Value::Array(self.content));
        Value::Object(value)
    }
}

fn insert_optional(value: &mut Map<String, Value>, name: &str, field: Option<Value>) {
    if let Some(field) = field {
        value.insert(name.to_owned(), field);
    }
}

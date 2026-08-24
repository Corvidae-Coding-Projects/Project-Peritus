use serde_json::{Map, Value};

pub(super) fn object<const N: usize>(entries: [(&str, Value); N]) -> Value {
    let mut map = Map::new();
    for (name, value) in entries {
        map.insert(name.to_owned(), value);
    }
    Value::Object(map)
}

pub(super) fn string(value: &str) -> Value {
    Value::String(value.to_owned())
}

pub(super) fn optional_string(value: Option<&str>) -> Value {
    value.map_or(Value::Null, string)
}

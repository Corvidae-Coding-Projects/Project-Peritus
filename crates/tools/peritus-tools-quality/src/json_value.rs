//! Explicit JSON value assembly for ordinary quality projections.

use serde_json::{Map, Value};

pub fn object<const N: usize>(fields: [(&str, Value); N]) -> Value {
    let mut value = Map::new();
    for (name, field) in fields {
        value.insert(name.to_owned(), field);
    }
    Value::Object(value)
}

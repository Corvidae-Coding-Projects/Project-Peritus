//! Shared manual Serde helpers for the version-one plugin protocol.

use serde_json::{Map, Value};

pub(super) fn serialize_tagged<S>(
    serializer: S,
    tag_name: &'static str,
    tag_value: &'static str,
    content_name: &'static str,
    content: Option<Value>,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let mut object = Map::new();
    object.insert(tag_name.to_owned(), Value::String(tag_value.to_owned()));
    if let Some(content) = content {
        object.insert(content_name.to_owned(), content);
    }
    serde::Serialize::serialize(&Value::Object(object), serializer)
}

pub(super) fn object_value<const N: usize>(fields: [(&'static str, Value); N]) -> Value {
    Value::Object(fields.into_iter().map(|(name, value)| (name.to_owned(), value)).collect())
}

pub(super) fn to_value<E, T>(value: &T) -> Result<Value, E>
where
    E: serde::ser::Error,
    T: serde::Serialize + ?Sized,
{
    serde_json::to_value(value).map_err(|error| E::custom(error.to_string()))
}

pub(super) fn tagged_parts(
    value: Value,
    tag_name: &'static str,
    content_name: &'static str,
) -> Result<(String, Option<Value>), String> {
    let Value::Object(mut object) = value else {
        return Err("tagged protocol value must be an object".to_owned());
    };
    if object.keys().any(|key| key != tag_name && key != content_name) {
        return Err("tagged protocol value contains an unknown field".to_owned());
    }
    let Some(Value::String(tag)) = object.remove(tag_name) else {
        return Err("tagged protocol value is missing a string tag".to_owned());
    };
    Ok((tag, object.remove(content_name)))
}

pub(super) fn decode_content<T, E>(content: Option<Value>, name: &'static str) -> Result<T, E>
where
    T: serde::de::DeserializeOwned,
    E: serde::de::Error,
{
    let value = content.ok_or_else(|| E::missing_field(name))?;
    serde_json::from_value(value).map_err(|error| E::custom(error.to_string()))
}

pub(super) fn unit_variant<E>(content: Option<&Value>, variant: &'static str) -> Result<(), E>
where
    E: serde::de::Error,
{
    if content.is_some() {
        Err(E::custom(format!("{variant} must not contain protocol content")))
    } else {
        Ok(())
    }
}

//! Private wire-format implementations for plugin manifests.

use std::fmt;

use serde::Deserialize;
use serde::Serialize;
use serde::{
    Deserializer, Serializer,
    de::{self, MapAccess, SeqAccess, Visitor},
    ser::SerializeStruct,
};

use super::{
    CapabilityDeclaration, PluginEntrypoint, PluginKind, PluginManifest, PluginOperation,
    PluginQuotas, ProtocolRange, SignatureDeclaration,
};

impl Serialize for PluginKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Self::Process => "process",
            Self::WasmComponent => "wasm-component",
        })
    }
}

impl<'de> Deserialize<'de> for PluginKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(PluginKindVisitor)
    }
}

struct PluginKindVisitor;

impl Visitor<'_> for PluginKindVisitor {
    type Value = PluginKind;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("`process` or `wasm-component`")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match value {
            "process" => Ok(PluginKind::Process),
            "wasm-component" => Ok(PluginKind::WasmComponent),
            _ => Err(E::unknown_variant(value, &["process", "wasm-component"])),
        }
    }
}

impl Serialize for PluginEntrypoint {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut value = serializer.serialize_struct("PluginEntrypoint", 2)?;
        value.serialize_field("artifact", &self.artifact)?;
        value.serialize_field("arguments", &self.arguments)?;
        value.end()
    }
}

impl<'de> Deserialize<'de> for PluginEntrypoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_struct(
            "PluginEntrypoint",
            &["artifact", "arguments"],
            PluginEntrypointVisitor,
        )
    }
}

struct PluginEntrypointVisitor;

impl<'de> Visitor<'de> for PluginEntrypointVisitor {
    type Value = PluginEntrypoint;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a plugin entrypoint with an artifact and optional arguments")
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let artifact =
            sequence.next_element()?.ok_or_else(|| de::Error::invalid_length(0, &self))?;
        let arguments = sequence.next_element()?.unwrap_or_default();
        Ok(PluginEntrypoint { artifact, arguments })
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut artifact = None;
        let mut arguments = None;
        while let Some(field) = map.next_key::<String>()? {
            match field.as_str() {
                "artifact" => {
                    if artifact.is_some() {
                        return Err(de::Error::duplicate_field("artifact"));
                    }
                    artifact = Some(map.next_value()?);
                }
                "arguments" => {
                    if arguments.is_some() {
                        return Err(de::Error::duplicate_field("arguments"));
                    }
                    arguments = Some(map.next_value()?);
                }
                _ => {
                    return Err(de::Error::unknown_field(&field, &["artifact", "arguments"]));
                }
            }
        }
        let artifact = artifact.ok_or_else(|| de::Error::missing_field("artifact"))?;
        Ok(PluginEntrypoint { artifact, arguments: arguments.unwrap_or_default() })
    }
}

impl Serialize for PluginOperation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(match self {
            Self::Inspection => "inspection",
            Self::WorkspaceMutation => "workspace-mutation",
            Self::Execution => "execution",
            Self::Network => "network",
            Self::SecretUse => "secret-use",
            Self::ExternalSideEffect => "external-side-effect",
        })
    }
}

impl<'de> Deserialize<'de> for PluginOperation {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(PluginOperationVisitor)
    }
}

struct PluginOperationVisitor;

impl Visitor<'_> for PluginOperationVisitor {
    type Value = PluginOperation;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a canonical kebab-case plugin operation")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match value {
            "inspection" => Ok(PluginOperation::Inspection),
            "workspace-mutation" => Ok(PluginOperation::WorkspaceMutation),
            "execution" => Ok(PluginOperation::Execution),
            "network" => Ok(PluginOperation::Network),
            "secret-use" => Ok(PluginOperation::SecretUse),
            "external-side-effect" => Ok(PluginOperation::ExternalSideEffect),
            _ => Err(E::unknown_variant(
                value,
                &[
                    "inspection",
                    "workspace-mutation",
                    "execution",
                    "network",
                    "secret-use",
                    "external-side-effect",
                ],
            )),
        }
    }
}

impl Serialize for CapabilityDeclaration {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut value = serializer.serialize_struct("CapabilityDeclaration", 3)?;
        value.serialize_field("name", &self.name)?;
        value.serialize_field("operation", &self.operation)?;
        value.serialize_field("required", &self.required)?;
        value.end()
    }
}

impl Serialize for ProtocolRange {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut value = serializer.serialize_struct("ProtocolRange", 2)?;
        value.serialize_field("minimum", &self.minimum)?;
        value.serialize_field("maximum", &self.maximum)?;
        value.end()
    }
}

impl Serialize for PluginQuotas {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut value = serializer.serialize_struct("PluginQuotas", 6)?;
        value.serialize_field("concurrent_requests", &self.concurrent_requests)?;
        value.serialize_field("frame_bytes", &self.frame_bytes)?;
        value.serialize_field("output_bytes", &self.output_bytes)?;
        value.serialize_field("invocation_millis", &self.invocation_millis)?;
        value.serialize_field("lifecycle_requests", &self.lifecycle_requests)?;
        value.serialize_field("protocol_violations", &self.protocol_violations)?;
        value.end()
    }
}

impl Serialize for SignatureDeclaration {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut value = serializer.serialize_struct("SignatureDeclaration", 3)?;
        value.serialize_field("key_id", &self.key_id)?;
        value.serialize_field("algorithm", &self.algorithm)?;
        value.serialize_field("signature", &self.signature)?;
        value.end()
    }
}

impl Serialize for PluginManifest {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let field_count = if self.signature.is_some() { 9 } else { 8 };
        let mut value = serializer.serialize_struct("PluginManifest", field_count)?;
        value.serialize_field("manifest_version", &self.manifest_version)?;
        value.serialize_field("id", &self.id)?;
        value.serialize_field("version", &self.version)?;
        value.serialize_field("kind", &self.kind)?;
        value.serialize_field("protocol", &self.protocol)?;
        value.serialize_field("entrypoint", &self.entrypoint)?;
        value.serialize_field("capabilities", &self.capabilities)?;
        value.serialize_field("quotas", &self.quotas)?;
        if let Some(signature) = &self.signature {
            value.serialize_field("signature", signature)?;
        }
        value.end()
    }
}

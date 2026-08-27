//! Manual MCP wire projections for bridge-owned domain observations.

use super::{
    BridgePrompt, BridgePromptArgument, BridgePromptMessage, BridgeResource,
    BridgeResourceContents, BridgeTool, BridgeToolCallResult, PromptTextContent, ToolTextContent,
};

impl serde::Serialize for BridgeTool {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(3))?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "name", &self.name)?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "description", &self.description)?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "inputSchema", &self.input_schema)?;
        serde::ser::SerializeMap::end(map)
    }
}

impl serde::Serialize for BridgeResource {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let fields =
            2 + usize::from(self.description.is_some()) + usize::from(self.mime_type.is_some());
        let mut map = serializer.serialize_map(Some(fields))?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "uri", &self.uri)?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "name", &self.name)?;
        if let Some(description) = &self.description {
            serde::ser::SerializeMap::serialize_entry(&mut map, "description", description)?;
        }
        if let Some(mime_type) = &self.mime_type {
            serde::ser::SerializeMap::serialize_entry(&mut map, "mimeType", mime_type)?;
        }
        serde::ser::SerializeMap::end(map)
    }
}

impl serde::Serialize for BridgeResourceContents {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let fields = 1
            + usize::from(self.mime_type.is_some())
            + usize::from(self.text.is_some())
            + usize::from(self.blob.is_some());
        let mut map = serializer.serialize_map(Some(fields))?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "uri", &self.uri)?;
        if let Some(mime_type) = &self.mime_type {
            serde::ser::SerializeMap::serialize_entry(&mut map, "mimeType", mime_type)?;
        }
        if let Some(text) = &self.text {
            serde::ser::SerializeMap::serialize_entry(&mut map, "text", text)?;
        }
        if let Some(blob) = &self.blob {
            serde::ser::SerializeMap::serialize_entry(&mut map, "blob", blob)?;
        }
        serde::ser::SerializeMap::end(map)
    }
}

impl serde::Serialize for BridgePromptArgument {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map =
            serializer.serialize_map(Some(2 + usize::from(self.description.is_some())))?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "name", &self.name)?;
        if let Some(description) = &self.description {
            serde::ser::SerializeMap::serialize_entry(&mut map, "description", description)?;
        }
        serde::ser::SerializeMap::serialize_entry(&mut map, "required", &self.required)?;
        serde::ser::SerializeMap::end(map)
    }
}

impl serde::Serialize for BridgePrompt {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map =
            serializer.serialize_map(Some(2 + usize::from(self.description.is_some())))?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "name", &self.name)?;
        if let Some(description) = &self.description {
            serde::ser::SerializeMap::serialize_entry(&mut map, "description", description)?;
        }
        serde::ser::SerializeMap::serialize_entry(&mut map, "arguments", &self.arguments)?;
        serde::ser::SerializeMap::end(map)
    }
}

impl serde::Serialize for BridgePromptMessage {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "role", &self.role)?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "content", &self.content)?;
        serde::ser::SerializeMap::end(map)
    }
}

impl serde::Serialize for PromptTextContent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "type", self.content_type)?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "text", &self.text)?;
        serde::ser::SerializeMap::end(map)
    }
}

impl serde::Serialize for BridgeToolCallResult {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map =
            serializer.serialize_map(Some(2 + usize::from(self.structured_content.is_some())))?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "content", &self.content)?;
        if let Some(structured) = &self.structured_content {
            serde::ser::SerializeMap::serialize_entry(&mut map, "structuredContent", structured)?;
        }
        serde::ser::SerializeMap::serialize_entry(&mut map, "isError", &self.is_error)?;
        serde::ser::SerializeMap::end(map)
    }
}

impl serde::Serialize for ToolTextContent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let mut map = serializer.serialize_map(Some(2))?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "type", self.content_type)?;
        serde::ser::SerializeMap::serialize_entry(&mut map, "text", &self.text)?;
        serde::ser::SerializeMap::end(map)
    }
}

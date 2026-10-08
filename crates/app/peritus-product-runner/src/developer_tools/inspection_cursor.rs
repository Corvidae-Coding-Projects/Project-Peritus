//! Scope-bound continuation cursors for bounded filesystem listings and searches.

use std::path::Path;

use peritus_agent::DeveloperLoopError;
use serde_json::Value;
use sha2::{Digest as _, Sha256};

use super::{path::tool, wire::object};

#[derive(Clone, Copy)]
pub struct CursorScope([u8; 32]);

impl CursorScope {
    pub(crate) fn new(operation: &str, root: &Path, path: &Path) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(b"peritus-workspace-cursor-v1\0");
        push(&mut hasher, operation.as_bytes());
        push(&mut hasher, root.as_os_str().as_encoded_bytes());
        push(&mut hasher, path.as_os_str().as_encoded_bytes());
        Self(hasher.finalize().into())
    }

    pub(crate) fn add_text(self, value: &str) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(self.0);
        push(&mut hasher, value.as_bytes());
        Self(hasher.finalize().into())
    }

    pub(crate) fn add_usize(self, value: usize) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(self.0);
        hasher.update(u64::try_from(value).unwrap_or(u64::MAX).to_le_bytes());
        Self(hasher.finalize().into())
    }

    pub(crate) fn read(self, arguments: &Value) -> Result<usize, DeveloperLoopError> {
        let Some(value) = arguments.get("cursor") else { return Ok(0) };
        let cursor = value
            .as_object()
            .ok_or_else(|| tool("cursor must be a continuation returned for this operation"))?;
        if cursor.get("scope").and_then(Value::as_str) != Some(self.hex().as_str()) {
            return Err(tool("continuation cursor belongs to a different path or query"));
        }
        let ordinal = cursor
            .get("ordinal")
            .and_then(Value::as_u64)
            .ok_or_else(|| tool("continuation cursor has no valid ordinal"))?;
        usize::try_from(ordinal).map_err(|_| tool("continuation cursor ordinal exceeds this host"))
    }

    pub(crate) fn value(self, ordinal: usize) -> Value {
        object(vec![("scope", Value::String(self.hex())), ("ordinal", Value::from(ordinal))])
    }

    fn hex(self) -> String {
        let mut value = String::with_capacity(self.0.len() * 2);
        for byte in self.0 {
            use std::fmt::Write as _;
            let _ = write!(value, "{byte:02x}");
        }
        value
    }
}

fn push(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update(u64::try_from(bytes.len()).unwrap_or(u64::MAX).to_le_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn continuation_cursors_are_bound_to_path_and_query_scope() {
        let root = Path::new("/workspace");
        let path_scope = CursorScope::new("workspace-search", root, Path::new("src"))
            .add_text("src")
            .add_text("needle");
        let cursor = path_scope.value(12);

        assert_eq!(path_scope.read(&json!({"cursor":cursor})).unwrap(), 12);
        assert!(
            CursorScope::new("workspace-search", root, Path::new("tests"))
                .add_text("tests")
                .add_text("needle")
                .read(&json!({"cursor":path_scope.value(12)}))
                .is_err()
        );
        assert!(
            CursorScope::new("workspace-search", root, Path::new("src"))
                .add_text("src")
                .add_text("different")
                .read(&json!({"cursor":path_scope.value(12)}))
                .is_err()
        );
    }
}

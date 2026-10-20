//! Immutable skill packages discovered under the repository's canonical `skills/` at build time.
//!
//! Adding a package or a resource needs no Rust registration. Resource paths are exact portable
//! relative names; lookup never accesses the runtime filesystem or executes bundled scripts.

/// One compiled-in skill and its complete resource inventory.
#[derive(Debug)]
pub struct BundledSkill {
    name: &'static str,
    description: &'static str,
    resources: &'static [BundledResource],
}

/// An immutable compiled-in file, including binary resources.
#[derive(Debug)]
pub struct BundledResource {
    path: &'static str,
    bytes: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/bundled_skills.rs"));

impl BundledSkill {
    /// Returns the canonical directory name used by `skill_read`.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Returns the description from `SKILL.md` metadata, or its first heading.
    #[must_use]
    pub const fn description(&self) -> &'static str {
        self.description
    }

    /// Lists all files in deterministic relative-path order.
    #[must_use]
    pub const fn resources(&self) -> &'static [BundledResource] {
        self.resources
    }
}

impl BundledResource {
    /// Returns the exact slash-separated path within its skill package.
    #[must_use]
    pub const fn path(&self) -> &'static str {
        self.path
    }

    /// Returns the original file bytes without decoding or normalization.
    #[must_use]
    pub const fn bytes(&self) -> &'static [u8] {
        self.bytes
    }
}

/// Lists every skill embedded in this build, in directory-name order.
#[must_use]
pub const fn catalog() -> &'static [BundledSkill] {
    SKILLS
}

/// Looks up original resource bytes by exact skill name and relative path.
#[must_use]
pub fn resource(skill: &str, path: &str) -> Option<&'static [u8]> {
    catalog()
        .iter()
        .find(|entry| entry.name == skill)?
        .resources
        .iter()
        .find(|entry| entry.path == path)
        .map(|entry| entry.bytes)
}

/// Reads an exact resource page through the same reader used by the model's `skill_read` tool.
/// Text offsets must be UTF-8 boundaries; binary pages use hexadecimal with original byte offsets.
/// `max_bytes` includes the serialized response metadata.
///
/// # Errors
/// Rejects unknown resources, invalid offsets, and page budgets outside 256..=524288 bytes or
/// too small to make forward progress.
pub fn read_page(
    skill: &str,
    path: &str,
    offset: u64,
    max_bytes: usize,
) -> Result<serde_json::Value, crate::ProductRunnerError> {
    page_budget(max_bytes)?;
    crate::developer_tools::read_bundled_skill(&serde_json::json!({
        "skill": skill, "path": path, "offset": offset, "max_bytes": max_bytes
    }))
    .map_err(page_error)
}

/// Reads a paginated JSON catalog of skills, or resource metadata for one named skill.
/// The `bytes` field contains exact catalog JSON text; follow `next_offset` to reconstruct it.
///
/// # Errors
/// Rejects unknown skills, invalid offsets, and page budgets outside 256..=524288 bytes or too
/// small to make forward progress.
pub fn catalog_page(
    skill: Option<&str>,
    offset: u64,
    max_bytes: usize,
) -> Result<serde_json::Value, crate::ProductRunnerError> {
    page_budget(max_bytes)?;
    let mut arguments = serde_json::json!({"offset": offset, "max_bytes": max_bytes});
    if let Some(skill) = skill {
        arguments["skill"] = serde_json::Value::from(skill);
    }
    crate::developer_tools::list_bundled_skills(&arguments).map_err(page_error)
}

fn page_budget(maximum: usize) -> Result<(), crate::ProductRunnerError> {
    if (256..=524_288).contains(&maximum) {
        Ok(())
    } else {
        Err(page_error("max_bytes must be between 256 and 524288"))
    }
}

fn page_error(error: impl std::fmt::Display) -> crate::ProductRunnerError {
    crate::ProductRunnerError::new(
        crate::ProductRunnerErrorKind::InvalidPrecondition,
        "read bundled skill page",
        error.to_string(),
    )
}

pub(crate) fn instructions(skill: &str) -> &'static str {
    resource(skill, "SKILL.md").and_then(|bytes| std::str::from_utf8(bytes).ok()).unwrap_or("")
}

//! Strict manifest validation and immutable package identity.
use crate::{Error, invalid};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Component, Path, PathBuf},
};

/// Module identity, role instructions and ordered data dependencies.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Format version, currently one.
    pub schema: u32,
    /// Stable lower-case module identifier.
    pub id: String,
    /// Numeric major.minor.patch version.
    pub version: String,
    /// Human-readable name.
    pub name: String,
    /// Purpose shown before selection.
    pub description: String,
    /// Host roles; instructions are subordinate to the user and host policy.
    #[serde(default)]
    pub roles: Vec<Role>,
    /// Stages in execution order. Dependencies must precede the consumer.
    #[serde(default)]
    pub stages: Vec<Stage>,
    /// Optional package-relative, immutable read resources.
    #[serde(default)]
    pub resources: Vec<String>,
    /// Optional executable code plugin. Absent for older declarative workflows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<Code>,
}
/// Native Rust plugin artifacts and maximum borrowed host tool capabilities.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Code {
    /// Stable JSON command ABI version, currently one.
    pub abi: u32,
    /// Target triple to package-relative native executable, all declared as resources.
    pub artifacts: std::collections::BTreeMap<String, String>,
    /// Maximum native read-only host tools that this plugin may request for its agents.
    pub tools: Vec<String>,
}
/// One separately scoped provider invocation policy.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Role {
    /// Unique role name.
    pub id: String,
    /// Existing selected provider role: writer or reviewer.
    pub model: String,
    /// Instructions for this role only.
    pub instructions: String,
    /// Exact read-only host tool names made available to this role.
    pub tools: Vec<String>,
}
/// An ordered stage and explicit predecessor outputs it may consume.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Stage {
    /// Unique stage name.
    pub id: String,
    /// Declared role to invoke.
    pub role: String,
    /// Earlier stage IDs, never implicit access to all prior output.
    #[serde(default)]
    pub inputs: Vec<String>,
}
/// Validated exact native code or declarative workflow package.
#[derive(Clone, Debug)]
pub struct Definition {
    /// Validated public module metadata.
    pub manifest: Manifest,
    /// Digest binds canonical manifest and every declared resource's path and bytes.
    pub digest: String,
    /// Immutable installed package root, absent for a resource-free bundled package.
    pub root: Option<PathBuf>,
    /// Whether the definition comes from the application bundle.
    pub bundled: bool,
}
impl Manifest {
    /// Parses and validates a module without executing it.
    pub fn parse(text: &str) -> Result<Self, Error> {
        if text.len() > 256 * 1024 {
            return Err(invalid("module manifest exceeds 256 KiB"));
        }
        let value: Self = toml::from_str(text)?;
        value.validate()?;
        Ok(value)
    }
    /// Checks identities, model selection and stage dependency routing.
    pub fn validate(&self) -> Result<(), Error> {
        if self.schema != 1
            || !identifier(&self.id)
            || matches!(self.id.as_str(), "resume" | "show" | "status")
            || self.name.chars().any(char::is_control)
            || self.description.chars().any(char::is_control)
            || !version(&self.version)
            || self.name.trim().is_empty()
            || self.description.trim().is_empty()
            || (self.code.is_none() && (self.roles.is_empty() || self.stages.is_empty()))
            || self.roles.len() > 128
            || self.stages.len() > 128
            || self.resources.len() > 1024
        {
            return Err(invalid(
                "invalid module identity/version, metadata or role/stage inventory",
            ));
        }
        let mut roles = BTreeSet::new();
        for role in &self.roles {
            if !identifier(&role.id)
                || !roles.insert(&role.id)
                || !matches!(role.model.as_str(), "writer" | "reviewer")
                || role.instructions.trim().is_empty()
                || role.instructions.contains('\0')
                || role.tools.iter().any(|t| !tool_name(t))
                || role.tools.len() > 128
            {
                return Err(invalid(
                    "invalid/repeated module role, model, instructions or tool names",
                ));
            }
        }
        let mut stages = BTreeSet::new();
        for stage in &self.stages {
            if !identifier(&stage.id)
                || !roles.contains(&stage.role)
                || stage.inputs.iter().any(|id| !stages.contains(id))
                || stage.inputs.iter().collect::<BTreeSet<_>>().len() != stage.inputs.len()
                || !stages.insert(stage.id.clone())
            {
                return Err(invalid(
                    "stage identity/role is invalid or dependencies repeat, cycle or do not precede it",
                ));
            }
        }
        let mut resources = BTreeSet::new();
        for resource in &self.resources {
            safe_relative(resource)?;
            if !resources.insert(resource) {
                return Err(invalid("repeated module resource"));
            }
        }
        if let Some(code) = &self.code {
            for artifact in code.artifacts.values() {
                safe_relative(artifact)?;
            }
            if code.abi != 1
                || self.version.split('.').any(|p| p.parse::<u16>().is_err())
                || code.artifacts.is_empty()
                || code.artifacts.keys().any(|t| {
                    t.is_empty()
                        || t.len() > 128
                        || !t.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
                })
                || !self.roles.is_empty()
                || !self.stages.is_empty()
                || code.artifacts.values().any(|a| !self.resources.contains(a))
                || code.tools.iter().any(|t| !tool_name(t))
                || code.tools.iter().collect::<BTreeSet<_>>().len() != code.tools.len()
            {
                return Err(invalid(
                    "native code modules require ABI 1, declared native executable resources, unique host tools, and no host-interpreted roles/stages",
                ));
            }
        }
        Ok(())
    }
}
impl Definition {
    /// Loads a local package. All resources must be regular files without symlink traversal.
    pub fn from_directory(root: &Path) -> Result<Self, Error> {
        if fs::symlink_metadata(root)?.file_type().is_symlink() {
            return Err(invalid("module root cannot be a symlink"));
        }
        let root = root.canonicalize()?;
        let path = checked_file(&root, "module.toml")?;
        if fs::metadata(&path)?.len() > 256 * 1024 {
            return Err(invalid("module manifest exceeds 256 KiB"));
        }
        let manifest = Manifest::parse(&fs::read_to_string(path)?)?;
        Self::assemble(manifest, Some(root), false)
    }
    pub(crate) fn bundled(text: &str) -> Result<Self, Error> {
        Self::assemble(Manifest::parse(text)?, None, true)
    }
    fn assemble(manifest: Manifest, root: Option<PathBuf>, bundled: bool) -> Result<Self, Error> {
        let mut hash = Sha256::new();
        hash.update(b"peritus-module-v1\0");
        hash.update(serde_json::to_vec(&manifest)?);
        let mut resources = manifest.resources.clone();
        resources.sort();
        for name in resources {
            hash.update((name.len() as u64).to_be_bytes());
            hash.update(name.as_bytes());
            let path = checked_file(
                root.as_deref()
                    .ok_or_else(|| invalid("bundled resources require an owned root"))?,
                &name,
            )?;
            let mut file = fs::File::open(path)?;
            let size = file.metadata()?.len();
            hash.update(size.to_be_bytes());
            let mut buffer = [0u8; 32768];
            loop {
                let count = file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                hash.update(&buffer[..count]);
            }
        }
        let digest = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
        Ok(Self { manifest, digest, root, bundled })
    }
    /// Resolves an explicitly declared resource for a paged host read.
    pub fn resource(&self, name: &str) -> Result<PathBuf, Error> {
        if !self.manifest.resources.iter().any(|r| r == name) {
            return Err(invalid("resource is not declared by this module"));
        }
        checked_file(
            self.root.as_deref().ok_or_else(|| invalid("bundled module has no file resources"))?,
            name,
        )
    }
}
pub(crate) fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 96
        && value.as_bytes()[0].is_ascii_lowercase()
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'.'))
        && !value.contains("..")
        && !value.ends_with('.')
}
fn tool_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
pub(crate) fn version(value: &str) -> bool {
    let parts: Vec<_> = value.split('.').collect();
    parts.len() == 3
        && parts.iter().all(|p| {
            !p.is_empty()
                && (p.len() == 1 || !p.starts_with('0'))
                && p.bytes().all(|b| b.is_ascii_digit())
                && p.parse::<u64>().is_ok()
        })
}
fn safe_relative(value: &str) -> Result<(), Error> {
    if value.is_empty()
        || value.contains('\\')
        || !Path::new(value).components().all(|p| matches!(p, Component::Normal(_)))
    {
        return Err(invalid("module resource must be a safe relative file path"));
    }
    Ok(())
}
pub(crate) fn checked_file(root: &Path, name: &str) -> Result<PathBuf, Error> {
    safe_relative(name)?;
    let mut path = root.to_path_buf();
    for part in Path::new(name).components() {
        path.push(part);
        if fs::symlink_metadata(&path)?.file_type().is_symlink() {
            return Err(invalid("module files cannot traverse symlinks"));
        }
    }
    if !fs::metadata(&path)?.is_file() {
        return Err(invalid("module resource is not a regular file"));
    }
    Ok(path)
}

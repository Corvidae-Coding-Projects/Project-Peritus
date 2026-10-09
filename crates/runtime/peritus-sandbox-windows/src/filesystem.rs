//! Windows path identity, reparse checks, and exact temporary ACL policy.

mod acl;
mod path;

pub use acl::{AclAccess, AclEntry, AclPlan, AclTransaction};
pub use path::{PathEvidence, ResolvedWindowsPath, WindowsPath};

use peritus_sandbox::{CheckedSandboxPlan, FileOperation, PathScope, RuleEffect};

use crate::{WindowsError, WindowsOperation, error};

/// Immutable workspace and protected-metadata path policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathPolicy {
    workspace: WindowsPath,
    protected_roots: Vec<WindowsPath>,
    read_only_inputs: Vec<WindowsPath>,
    writable_inputs: Vec<WindowsPath>,
}

impl PathPolicy {
    /// Creates a canonical workspace policy.
    ///
    /// # Errors
    /// Rejects distinct spellings that alias the same native protected root.
    pub fn new(
        workspace: WindowsPath,
        mut protected_roots: Vec<WindowsPath>,
    ) -> Result<Self, WindowsError> {
        normalize_paths(&mut protected_roots, "protected roots contain a case-fold alias")?;
        Ok(Self {
            workspace,
            protected_roots,
            read_only_inputs: Vec::new(),
            writable_inputs: Vec::new(),
        })
    }

    /// Admits explicitly installed external inputs for read/execute rules only.
    /// This does not add ACL permissions; each actual permission still requires a checked rule.
    ///
    /// # Errors
    /// Rejects distinct spellings that alias one native input root.
    pub fn with_read_only_inputs(
        mut self,
        mut inputs: Vec<WindowsPath>,
    ) -> Result<Self, WindowsError> {
        normalize_paths(&mut inputs, "read-only inputs contain a case-fold alias")?;
        self.read_only_inputs = inputs;
        Ok(self)
    }

    /// Admits exact external roots for checked read/write rules.
    ///
    /// The roots carry no ambient permission. They only make matching filesystem rules
    /// representable by the Windows ACL projection.
    ///
    /// # Errors
    /// Rejects ambiguous native aliases. Read-only and protected domains remain deny-dominant.
    pub fn with_writable_inputs(
        mut self,
        mut inputs: Vec<WindowsPath>,
    ) -> Result<Self, WindowsError> {
        normalize_paths(&mut inputs, "writable inputs contain a case-fold alias")?;
        self.writable_inputs = inputs;
        Ok(self)
    }

    fn resolve_rule(
        &self,
        rule: &peritus_sandbox::FilesystemRule,
    ) -> Result<AuthorizedPath, WindowsError> {
        let path = WindowsPath::from_sandbox(&self.workspace, rule.path())?;
        let writes = [FileOperation::Create, FileOperation::Write, FileOperation::Remove]
            .into_iter()
            .any(|operation| rule.operations().contains(operation));
        if rule.effect() == RuleEffect::Allow
            && writes
            && self.read_only_inputs.iter().any(|root| root.contains(&path))
        {
            return Err(error::invalid(
                WindowsOperation::ResolvePath,
                "allow writes overlap a configured read-only domain",
            ));
        }
        if self.workspace.contains(&path) {
            return Ok(AuthorizedPath { path, root: self.workspace.clone(), writable: true });
        }
        let writes = [FileOperation::Create, FileOperation::Write, FileOperation::Remove]
            .into_iter()
            .any(|operation| rule.operations().contains(operation));
        if let Some(root) = most_specific_root(&self.writable_inputs, &path) {
            return Ok(AuthorizedPath { path, root: root.clone(), writable: true });
        }
        if !writes && let Some(root) = most_specific_root(&self.read_only_inputs, &path) {
            return Ok(AuthorizedPath { path, root: root.clone(), writable: false });
        }
        Err(error::invalid(
            WindowsOperation::ResolvePath,
            "rule escapes workspace or writes an external input",
        ))
    }

    /// Returns the normalized workspace root.
    #[must_use]
    pub const fn workspace(&self) -> &WindowsPath {
        &self.workspace
    }

    /// Returns canonical protected roots.
    #[must_use]
    pub fn protected_roots(&self) -> &[WindowsPath] {
        &self.protected_roots
    }

    /// Returns canonical external read-only roots.
    #[must_use]
    pub fn read_only_inputs(&self) -> &[WindowsPath] {
        &self.read_only_inputs
    }

    /// Returns canonical external writable roots.
    #[must_use]
    pub fn writable_inputs(&self) -> &[WindowsPath] {
        &self.writable_inputs
    }

    /// Maps one platform-neutral sandbox path into this workspace.
    ///
    /// # Errors
    /// Rejects another volume, a path outside the workspace, or a protected metadata overlap.
    pub fn resolve_logical(
        &self,
        logical: &peritus_sandbox::SandboxPath,
    ) -> Result<WindowsPath, WindowsError> {
        let path = WindowsPath::from_sandbox(&self.workspace, logical)?;
        if !self.workspace.contains(&path) {
            return Err(error::invalid(
                WindowsOperation::ResolvePath,
                "sandbox path escapes the exact workspace",
            ));
        }
        Ok(path)
    }
}

/// Compiles each C2 filesystem operation into Windows-specific ACL access.
///
/// Deny rules and protected metadata remain explicit deny entries. No broad recursive ACL
/// mutation is generated.
///
/// # Errors
/// Rejects ambiguous aliases or writes whose exact target is in a read-only domain.
pub fn compile_acl_plan(
    plan: &CheckedSandboxPlan,
    policy: &PathPolicy,
    principal_sid: &str,
) -> Result<AclPlan, WindowsError> {
    let required_capacity = plan
        .contract()
        .filesystem()
        .rules()
        .len()
        .checked_add(policy.protected_roots().len())
        .ok_or_else(|| {
            error::invalid(WindowsOperation::CompileAcl, "ACL path capacity overflows")
        })?;
    let mut projected = Vec::new();
    projected.try_reserve_exact(required_capacity).map_err(|_| {
        error::invalid(WindowsOperation::CompileAcl, "ACL path capacity is unavailable")
    })?;
    for rule in plan.contract().filesystem().rules() {
        let authorized = policy.resolve_rule(rule)?;
        let mut access = AclAccess::empty();
        for operation in FILE_OPERATIONS {
            if rule.operations().contains(operation) {
                access.insert(operation);
            }
        }
        let create_deny_directory = rule.effect() == RuleEffect::Deny
            && rule.scope() == PathScope::Descendants
            && authorized.writable;
        projected.push(AclEntry::new_authorized(
            rule.effect(),
            authorized.path,
            rule.scope(),
            access,
            authorized.root,
            create_deny_directory,
        )?);
    }
    append_protected_denies(policy, &mut projected)?;
    let mut denies = Vec::new();
    denies.try_reserve_exact(projected.len()).map_err(|_| {
        error::invalid(WindowsOperation::CompileAcl, "ACL deny capacity is unavailable")
    })?;
    denies.extend(projected.iter().filter(|entry| entry.effect() == RuleEffect::Deny).cloned());
    let mut entries = Vec::new();
    entries.try_reserve_exact(projected.len()).map_err(|_| {
        error::invalid(WindowsOperation::CompileAcl, "ACL entry capacity is unavailable")
    })?;
    for mut entry in projected {
        if entry.effect() == RuleEffect::Allow {
            for deny in &denies {
                entry.subtract_inherited_deny(deny);
            }
            if entry.access().bits() == 0 {
                continue;
            }
        }
        entries.push(entry);
    }
    AclPlan::new(principal_sid, entries)
}

fn append_protected_denies(
    policy: &PathPolicy,
    projected: &mut Vec<AclEntry>,
) -> Result<(), WindowsError> {
    let domains = std::iter::once(&policy.workspace)
        .chain(policy.read_only_inputs.iter())
        .chain(policy.writable_inputs.iter())
        .collect::<Vec<_>>();
    let mut protected_targets = Vec::new();
    for protected in policy.protected_roots() {
        for domain in &domains {
            if domain.contains(protected) {
                protected_targets.push(protected.clone());
            } else if protected.contains(domain) {
                protected_targets.push((*domain).clone());
            }
        }
    }
    normalize_paths(&mut protected_targets, "protected intersections contain a native alias")?;
    for protected in protected_targets {
        let authority = domains
            .iter()
            .copied()
            .filter(|domain| domain.contains(&protected))
            .max_by_key(|domain| domain.as_str().len())
            .ok_or_else(|| {
                error::invalid(
                    WindowsOperation::CompileAcl,
                    "protected intersection lacks a configured authority root",
                )
            })?;
        let writable = (policy.workspace.contains(&protected)
            || policy.writable_inputs.iter().any(|root| root.contains(&protected)))
            && !policy.read_only_inputs.iter().any(|root| root.contains(&protected));
        projected.push(AclEntry::new_authorized(
            RuleEffect::Deny,
            protected,
            PathScope::Descendants,
            AclAccess::all(),
            authority.clone(),
            writable,
        )?);
    }
    // Read-only configured domains remain deny-dominant even beneath a broad writable rule.
    for input in policy.read_only_inputs() {
        let mut access = AclAccess::empty();
        for operation in [FileOperation::Write, FileOperation::Create, FileOperation::Remove] {
            access.insert(operation);
        }
        projected.push(AclEntry::new_authorized(
            RuleEffect::Deny,
            input.clone(),
            PathScope::Descendants,
            access,
            input.clone(),
            false,
        )?);
    }
    Ok(())
}

#[derive(Clone, Debug)]
struct AuthorizedPath {
    path: WindowsPath,
    root: WindowsPath,
    writable: bool,
}

fn normalize_paths(
    paths: &mut Vec<WindowsPath>,
    alias_detail: &'static str,
) -> Result<(), WindowsError> {
    paths.sort_by(WindowsPath::stable_native_cmp);
    if paths.windows(2).any(|pair| pair[0].same_native_path(&pair[1]) && pair[0] != pair[1]) {
        return Err(error::invalid(WindowsOperation::Validate, alias_detail));
    }
    paths.dedup();
    Ok(())
}

fn most_specific_root<'a>(roots: &'a [WindowsPath], path: &WindowsPath) -> Option<&'a WindowsPath> {
    roots
        .iter()
        .filter(|root| root.contains(path))
        .max_by(|left, right| left.as_str().len().cmp(&right.as_str().len()))
}

const FILE_OPERATIONS: [FileOperation; 7] = [
    FileOperation::Discover,
    FileOperation::Metadata,
    FileOperation::Read,
    FileOperation::Execute,
    FileOperation::Create,
    FileOperation::Write,
    FileOperation::Remove,
];

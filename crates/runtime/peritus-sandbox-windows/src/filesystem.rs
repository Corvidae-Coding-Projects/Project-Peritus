//! Windows path identity, reparse checks, and exact temporary ACL policy.

mod acl;
mod path;

pub use acl::{AclAccess, AclEntry, AclPlan, AclTransaction};
pub use path::{PathEvidence, ResolvedWindowsPath, WindowsPath};

use peritus_sandbox::{CheckedSandboxPlan, FileOperation, PathScope, RuleEffect};

use crate::{WindowsError, WindowsOperation, error};

const MAX_PROTECTED_ROOTS: usize = 256;

/// Immutable workspace and protected-metadata path policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PathPolicy {
    workspace: WindowsPath,
    protected_roots: Vec<WindowsPath>,
}

impl PathPolicy {
    /// Creates a canonical workspace policy.
    ///
    /// # Errors
    /// Rejects excessive roots, a protected root on another volume, case aliases, or a protected
    /// path outside the workspace.
    pub fn new(
        workspace: WindowsPath,
        mut protected_roots: Vec<WindowsPath>,
    ) -> Result<Self, WindowsError> {
        if protected_roots.len() > MAX_PROTECTED_ROOTS {
            return Err(error::invalid(
                WindowsOperation::Validate,
                "protected root count exceeds its bound",
            ));
        }
        protected_roots.sort_by(|left, right| left.case_folded().cmp(right.case_folded()));
        for pair in protected_roots.windows(2) {
            if pair[0].case_folded() == pair[1].case_folded() && pair[0] != pair[1] {
                return Err(error::invalid(
                    WindowsOperation::Validate,
                    "protected roots contain a case-fold alias",
                ));
            }
        }
        protected_roots.dedup();
        if protected_roots.iter().any(|root| !workspace.contains(root)) {
            return Err(error::invalid(
                WindowsOperation::Validate,
                "protected root is outside the exact workspace volume",
            ));
        }
        Ok(Self { workspace, protected_roots })
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

    fn is_protected(&self, path: &WindowsPath) -> bool {
        self.protected_roots.iter().any(|root| root.contains(path) || path.contains(root))
    }
}

/// Compiles each C2 filesystem operation into Windows-specific ACL access.
///
/// Deny rules and protected metadata remain explicit deny entries. No broad recursive ACL
/// mutation is generated.
///
/// # Errors
/// Rejects aliases, protected allow overlap, or an operation Windows ACLs cannot represent.
pub fn compile_acl_plan(
    plan: &CheckedSandboxPlan,
    policy: &PathPolicy,
    principal_sid: &str,
) -> Result<AclPlan, WindowsError> {
    let mut entries = Vec::new();
    for rule in plan.contract().filesystem().rules() {
        let path = policy.resolve_logical(rule.path())?;
        if rule.effect() == RuleEffect::Allow && policy.is_protected(&path) {
            return Err(error::invalid(
                WindowsOperation::CompileAcl,
                "writable/readable rule overlaps protected metadata",
            ));
        }
        let mut access = AclAccess::empty();
        for operation in FILE_OPERATIONS {
            if rule.operations().contains(operation) {
                access.insert(operation);
            }
        }
        entries.push(AclEntry::new(rule.effect(), path, rule.scope(), access)?);
    }
    for protected in policy.protected_roots() {
        entries.push(AclEntry::new(
            RuleEffect::Deny,
            protected.clone(),
            PathScope::Descendants,
            AclAccess::all(),
        )?);
    }
    AclPlan::new(principal_sid, entries)
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

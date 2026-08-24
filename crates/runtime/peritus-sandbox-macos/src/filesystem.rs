//! Deterministic Seatbelt profile compilation.

use std::path::{Path, PathBuf};

use peritus_sandbox::{
    CheckedSandboxPlan, DescendantPolicy, FileDecision, FileOperation, FilesystemContract,
    FilesystemRule, PathScope, RuleEffect, SandboxPath, SecretDelivery, Transport,
};
use peritus_types::Sha256Digest;

use crate::{MacosError, MacosErrorKind, MacosOperation, ProxyRoute, RecoveryAction, error};

const PROFILE_VERSION: u16 = 1;
const MAX_PROFILE_BYTES: usize = 256 * 1_024;
const DEFAULT_PROTECTED_NAMES: [&str; 2] = [".git", ".peritus"];

/// Effective deterministic policy decision retained for compiler refinement tests.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileDecision {
    /// One allow rule matched and no deny rule or protected root matched.
    Allowed,
    /// A caller deny rule or backend-protected metadata root matched.
    DeniedExplicitly,
    /// No allow rule matched and the profile is deny by default.
    DeniedByDefault,
}

/// A complete, deterministic Seatbelt profile bound by its digest.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompiledSeatbeltProfile {
    text: String,
    digest: Sha256Digest,
    filesystem: FilesystemContract,
    protected_roots: Vec<SandboxPath>,
}

impl CompiledSeatbeltProfile {
    /// Returns complete profile text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the SHA-256 digest of the exact profile bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Evaluates the filesystem projection with protected-root and deny dominance.
    #[must_use]
    pub fn decide(&self, path: &SandboxPath, operation: FileOperation) -> ProfileDecision {
        if self.protected_roots.iter().any(|root| path_is_within(path, root)) {
            return ProfileDecision::DeniedExplicitly;
        }
        let (allow_matches, deny_matches) = match self.filesystem.decide(path, operation) {
            FileDecision::Allowed => (true, false),
            FileDecision::DeniedByRule => (false, true),
            FileDecision::DeniedByDefault => (false, false),
        };
        if crate::verified::deny_dominant(allow_matches, deny_matches) {
            ProfileDecision::Allowed
        } else if deny_matches {
            ProfileDecision::DeniedExplicitly
        } else {
            ProfileDecision::DeniedByDefault
        }
    }

    /// Returns canonical protected metadata roots.
    #[must_use]
    pub fn protected_roots(&self) -> &[SandboxPath] {
        &self.protected_roots
    }
}

/// Stateless deterministic compiler for the macOS Seatbelt policy language.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProfileCompiler;

impl ProfileCompiler {
    /// Compiles the complete C2 contract, mandatory metadata protection, process rules, and the
    /// optional managed proxy route.
    ///
    /// # Errors
    /// Fails closed when a path or policy cannot be represented exactly by this compiler.
    pub fn compile(
        plan: &CheckedSandboxPlan,
        workspace_root: &Path,
        additional_protected_roots: &[PathBuf],
        proxy: Option<ProxyRoute>,
    ) -> Result<CompiledSeatbeltProfile, MacosError> {
        let workspace_root_path = workspace_root;
        native_path(workspace_root_path)?;
        validate_filesystem_representability(plan.contract().filesystem())?;
        validate_network_representability(plan, proxy)?;

        let mut protected_roots = DEFAULT_PROTECTED_NAMES
            .iter()
            .map(|name| native_path(&workspace_root_path.join(name)))
            .collect::<Result<Vec<_>, _>>()?;
        protected_roots.extend(
            additional_protected_roots
                .iter()
                .map(|path| native_path(path))
                .collect::<Result<Vec<_>, _>>()?,
        );
        protected_roots.sort();
        protected_roots.dedup();

        let mut text = String::new();
        line(&mut text, &format!("(version {PROFILE_VERSION})"))?;
        line(&mut text, "(deny default)")?;
        if proxy.is_some() {
            line(&mut text, "(deny network-inbound)")?;
        } else {
            line(&mut text, "(deny network*)")?;
        }

        let executable = plan.requirements().process().program();
        line(&mut text, &format!("(allow process-exec (literal {}))", encode_path(executable)?))?;
        if matches!(plan.contract().process().descendants(), DescendantPolicy::Denied) {
            line(&mut text, "(deny process-fork)")?;
        } else {
            line(&mut text, "(allow process-fork)")?;
        }

        emit_filesystem_rules(&mut text, plan.contract().filesystem().rules(), RuleEffect::Allow)?;
        for requirement in plan.requirements().secrets() {
            if let SecretDelivery::File(path) = requirement.delivery() {
                let encoded = encode_path(path)?;
                line(&mut text, &format!("(allow file-read-metadata (literal {encoded}))"))?;
                line(&mut text, &format!("(allow file-read-data (literal {encoded}))"))?;
            }
        }
        emit_filesystem_rules(&mut text, plan.contract().filesystem().rules(), RuleEffect::Deny)?;

        for root in &protected_roots {
            let encoded = encode_path(root)?;
            // Protected runtime/repository metadata is stronger than a caller-level abstract
            // operation rule: deny every Seatbelt read/write sub-operation, including xattrs,
            // ownership/mode changes, links, and future family members, plus execution.
            for operation in ["file-read*", "file-write*", "process-exec"] {
                line(&mut text, &format!("(deny {operation} (subpath {encoded}))"))?;
            }
        }

        if let Some(route) = proxy {
            let remote = encode_string(&route.seatbelt_remote())?;
            line(&mut text, &format!("(allow network-outbound (remote tcp {remote}))"))?;
        }
        let digest = peritus_codec::sha256(text.as_bytes());
        Ok(CompiledSeatbeltProfile {
            text,
            digest,
            filesystem: plan.contract().filesystem().clone(),
            protected_roots,
        })
    }
}

fn native_path(path: &Path) -> Result<SandboxPath, MacosError> {
    let value = path.to_str().ok_or_else(|| {
        MacosError::new(
            MacosErrorKind::ProfileCompilation,
            MacosOperation::CompileProfile,
            RecoveryAction::CorrectRequest,
            "macOS sandbox path is not valid UTF-8",
        )
    })?;
    if value.bytes().any(|byte| byte.is_ascii_control()) {
        return Err(MacosError::new(
            MacosErrorKind::ProfileCompilation,
            MacosOperation::CompileProfile,
            RecoveryAction::CorrectRequest,
            "macOS sandbox path contains a control character",
        ));
    }
    SandboxPath::new(value).map_err(|_| {
        MacosError::new(
            MacosErrorKind::ProfileCompilation,
            MacosOperation::CompileProfile,
            RecoveryAction::CorrectRequest,
            "macOS sandbox path is not canonical absolute syntax",
        )
    })
}

fn validate_filesystem_representability(contract: &FilesystemContract) -> Result<(), MacosError> {
    for rule in contract.rules() {
        let discover = rule.operations().contains(FileOperation::Discover);
        let metadata = rule.operations().contains(FileOperation::Metadata);
        if discover != metadata {
            return Err(MacosError::new(
                MacosErrorKind::ProfileCompilation,
                MacosOperation::CompileProfile,
                RecoveryAction::SelectSupportedBackend,
                "Seatbelt cannot distinguish discovery from metadata for this rule",
            ));
        }
        encode_path(rule.path())?;
    }
    Ok(())
}

fn validate_network_representability(
    plan: &CheckedSandboxPlan,
    proxy: Option<ProxyRoute>,
) -> Result<(), MacosError> {
    let has_allowed_egress =
        plan.contract().network().rules().iter().any(|rule| rule.effect() == RuleEffect::Allow);
    if has_allowed_egress && proxy.is_none() {
        return Err(MacosError::new(
            MacosErrorKind::UnsupportedHost,
            MacosOperation::CompileProfile,
            RecoveryAction::SelectSupportedBackend,
            "allowed egress requires a checked managed proxy route",
        ));
    }
    if !has_allowed_egress && proxy.is_some() {
        return Err(MacosError::new(
            MacosErrorKind::PreparationMismatch,
            MacosOperation::CompileProfile,
            RecoveryAction::Reauthorize,
            "managed proxy route is broader than the checked network contract",
        ));
    }
    if plan
        .contract()
        .network()
        .rules()
        .iter()
        .any(|rule| rule.effect() == RuleEffect::Allow && rule.transport() == Transport::Udp)
    {
        return Err(MacosError::new(
            MacosErrorKind::UnsupportedHost,
            MacosOperation::CompileProfile,
            RecoveryAction::SelectSupportedBackend,
            "macOS managed proxy does not admit UDP",
        ));
    }
    Ok(())
}

fn emit_filesystem_rules(
    text: &mut String,
    rules: &[FilesystemRule],
    effect: RuleEffect,
) -> Result<(), MacosError> {
    let verb = if effect == RuleEffect::Allow { "allow" } else { "deny" };
    for rule in rules.iter().filter(|rule| rule.effect() == effect) {
        let filter = match rule.scope() {
            PathScope::Exact => "literal",
            PathScope::Descendants => "subpath",
        };
        let path = encode_path(rule.path())?;
        for operation in seatbelt_operations(rule.operations()) {
            line(text, &format!("({verb} {operation} ({filter} {path}))"))?;
        }
    }
    Ok(())
}

fn seatbelt_operations(operations: peritus_sandbox::FileOperationSet) -> Vec<&'static str> {
    let mut result = Vec::new();
    if operations.contains(FileOperation::Discover) || operations.contains(FileOperation::Metadata)
    {
        result.push("file-read-metadata");
    }
    if operations.contains(FileOperation::Read) {
        result.push("file-read-data");
    }
    if operations.contains(FileOperation::Execute) {
        result.push("process-exec");
    }
    if operations.contains(FileOperation::Create) {
        result.push("file-write-create");
    }
    if operations.contains(FileOperation::Write) {
        result.push("file-write-data");
    }
    if operations.contains(FileOperation::Remove) {
        result.push("file-write-unlink");
    }
    result.sort_unstable();
    result.dedup();
    result
}

fn encode_path(path: &SandboxPath) -> Result<String, MacosError> {
    encode_string(path.as_str())
}

fn encode_string(value: &str) -> Result<String, MacosError> {
    if value.bytes().any(|byte| byte == 0 || byte.is_ascii_control()) {
        return Err(MacosError::new(
            MacosErrorKind::ProfileCompilation,
            MacosOperation::CompileProfile,
            RecoveryAction::CorrectRequest,
            "Seatbelt string contains a control character",
        ));
    }
    let mut encoded = String::with_capacity(value.len() + 2);
    encoded.push('"');
    for character in value.chars() {
        match character {
            '"' => encoded.push_str("\\\""),
            '\\' => encoded.push_str("\\\\"),
            other => encoded.push(other),
        }
    }
    encoded.push('"');
    Ok(encoded)
}

fn line(text: &mut String, value: &str) -> Result<(), MacosError> {
    let next =
        text.len().checked_add(value.len()).and_then(|length| length.checked_add(1)).ok_or_else(
            || error::limited(MacosOperation::CompileProfile, "profile size overflow"),
        )?;
    if next > MAX_PROFILE_BYTES {
        return Err(error::limited(
            MacosOperation::CompileProfile,
            "Seatbelt profile exceeds its byte bound",
        ));
    }
    text.push_str(value);
    text.push('\n');
    Ok(())
}

fn path_is_within(candidate: &SandboxPath, root: &SandboxPath) -> bool {
    candidate == root
        || (candidate.as_str().starts_with(root.as_str())
            && candidate.as_str().as_bytes().get(root.as_str().len()) == Some(&b'/'))
}

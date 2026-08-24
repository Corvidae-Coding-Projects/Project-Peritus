//! Structured bubblewrap launch construction.

use crate::{
    HelperManifest, LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery, MountAction,
    MountPlan,
};
use peritus_process::CommandSpec;
use peritus_types::Sha256Digest;
use std::path::Path;

/// Protected manifest bytes supplied through inherited standard input by C2.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProtectedInput {
    bytes: Vec<u8>,
    digest: Sha256Digest,
}

impl ProtectedInput {
    /// Returns exact encoded manifest bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Returns their digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

/// Backend-local launch description adaptable to C2's process-owned type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinuxLaunchDescription {
    command: CommandSpec,
    helper_identity: String,
    manifest: ProtectedInput,
}

impl LinuxLaunchDescription {
    /// Builds a shell-free bubblewrap command. The target appears only in protected manifest bytes.
    ///
    /// # Errors
    /// Returns a typed error when paths or the process command cannot be represented.
    pub fn build(
        bubblewrap: &Path,
        helper: &Path,
        helper_digest: Sha256Digest,
        mounts: &MountPlan,
        manifest: &HelperManifest,
    ) -> Result<Self, LinuxError> {
        let bytes = manifest.encode()?;
        let digest = peritus_codec::sha256(&bytes);
        let mut arguments = vec![
            "--die-with-parent".to_owned(),
            "--new-session".to_owned(),
            "--unshare-user".to_owned(),
            "--unshare-pid".to_owned(),
            "--unshare-ipc".to_owned(),
            "--unshare-uts".to_owned(),
            "--unshare-net".to_owned(),
            "--cap-drop".to_owned(),
            "ALL".to_owned(),
            "--hostname".to_owned(),
            "peritus".to_owned(),
        ];
        for action in mounts.actions() {
            append_mount(&mut arguments, action);
        }
        for binding in manifest.protected_payloads() {
            if let peritus_sandbox::SecretDelivery::File(path) = binding.requirement().delivery() {
                arguments.push("--perms".to_owned());
                arguments.push("0400".to_owned());
                arguments.push("--ro-bind-data".to_owned());
                arguments.push(binding.handle().descriptor().to_string());
                arguments.push(path.as_str().to_owned());
            }
        }
        push_pair(&mut arguments, "--bind", manifest.cgroup_leaf(), manifest.cgroup_leaf());
        arguments.push("--chdir".to_owned());
        arguments.push(manifest.working_directory().to_string_lossy().into_owned());
        arguments.push("--".to_owned());
        arguments.push(helper.to_string_lossy().into_owned());
        arguments.push("--run".to_owned());
        arguments.push("--manifest-digest".to_owned());
        arguments.push(crate::canonical::digest_hex(digest));
        arguments.push("--preparation-digest".to_owned());
        arguments.push(crate::canonical::digest_hex(manifest.preparation_digest()));
        let command = CommandSpec::new(bubblewrap.to_string_lossy().into_owned(), arguments)
            .map_err(|_| {
                LinuxError::new(
                    LinuxErrorKind::Helper,
                    LinuxOperation::Prepare,
                    LinuxRecovery::CorrectRequest,
                    "bubblewrap command exceeds the process gateway bounds",
                )
            })?;
        Ok(Self {
            command,
            helper_identity: format!(
                "{}:{}:{}",
                crate::BACKEND_NAME,
                crate::BACKEND_VERSION,
                crate::canonical::digest_hex(helper_digest)
            ),
            manifest: ProtectedInput { bytes, digest },
        })
    }
    /// Returns the direct-child bubblewrap command.
    #[must_use]
    pub const fn command(&self) -> &CommandSpec {
        &self.command
    }
    /// Returns reviewed helper identity.
    #[must_use]
    pub fn helper_identity(&self) -> &str {
        &self.helper_identity
    }
    /// Returns protected manifest input.
    #[must_use]
    pub const fn manifest(&self) -> &ProtectedInput {
        &self.manifest
    }
}

fn append_mount(arguments: &mut Vec<String>, action: &MountAction) {
    match action {
        MountAction::ReadOnlyBind { source, target } => {
            push_pair(arguments, "--ro-bind", source, target);
        }
        MountAction::WritableBind { source, target } => {
            push_pair(arguments, "--bind", source, target);
        }
        MountAction::Proc { target } => push_single(arguments, "--proc", target),
        MountAction::Dev { target } => push_single(arguments, "--dev", target),
        MountAction::Tmpfs { target } => push_single(arguments, "--tmpfs", target),
        MountAction::Mask { target } if target.is_dir() => {
            push_single(arguments, "--tmpfs", target);
            push_single(arguments, "--remount-ro", target);
        }
        MountAction::Mask { target } => {
            push_pair(arguments, "--ro-bind", Path::new("/dev/null"), target);
        }
    }
}

fn push_pair(arguments: &mut Vec<String>, operation: &str, source: &Path, target: &Path) {
    arguments.push(operation.to_owned());
    arguments.push(source.to_string_lossy().into_owned());
    arguments.push(target.to_string_lossy().into_owned());
}

fn push_single(arguments: &mut Vec<String>, operation: &str, target: &Path) {
    arguments.push(operation.to_owned());
    arguments.push(target.to_string_lossy().into_owned());
}

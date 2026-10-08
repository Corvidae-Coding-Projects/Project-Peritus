//! Structured bubblewrap launch construction.

use crate::{
    HelperManifest, LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery, MountAction,
    MountPlan,
};
use peritus_process::CommandSpec;
use peritus_types::Sha256Digest;
use std::ffi::OsString;
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
        let mut arguments: Vec<OsString> = vec![
            "--die-with-parent".into(),
            "--new-session".into(),
            "--unshare-user".into(),
            "--unshare-pid".into(),
            "--unshare-ipc".into(),
            "--unshare-uts".into(),
            "--unshare-net".into(),
            "--cap-drop".into(),
            "ALL".into(),
            "--hostname".into(),
            "peritus".into(),
        ];
        for action in mounts.actions() {
            append_mount(&mut arguments, action);
        }
        for binding in manifest.protected_payloads() {
            if let peritus_sandbox::SecretDelivery::File(path) = binding.requirement().delivery() {
                arguments.push("--perms".into());
                arguments.push("0400".into());
                arguments.push("--ro-bind-data".into());
                arguments.push(binding.handle().descriptor().to_string().into());
                arguments.push(path.as_str().into());
            }
        }
        push_pair(&mut arguments, "--bind", manifest.cgroup_leaf(), manifest.cgroup_leaf());
        arguments.push("--chdir".into());
        arguments.push(manifest.working_directory().as_os_str().to_owned());
        arguments.push("--".into());
        arguments.push(helper.as_os_str().to_owned());
        arguments.push("--run".into());
        arguments.push("--manifest-digest".into());
        arguments.push(crate::canonical::digest_hex(digest).into());
        arguments.push("--preparation-digest".into());
        arguments.push(crate::canonical::digest_hex(manifest.preparation_digest()).into());
        let command = CommandSpec::new(bubblewrap.as_os_str().to_owned(), arguments)
            .map_err(|_| {
                LinuxError::new(
                    LinuxErrorKind::Helper,
                    LinuxOperation::Prepare,
                    LinuxRecovery::CorrectRequest,
                    "bubblewrap command is not a valid literal native command",
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

fn append_mount(arguments: &mut Vec<OsString>, action: &MountAction) {
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

fn push_pair(arguments: &mut Vec<OsString>, operation: &str, source: &Path, target: &Path) {
    arguments.push(operation.into());
    arguments.push(source.as_os_str().to_owned());
    arguments.push(target.as_os_str().to_owned());
}

fn push_single(arguments: &mut Vec<OsString>, operation: &str, target: &Path) {
    arguments.push(operation.into());
    arguments.push(target.as_os_str().to_owned());
}

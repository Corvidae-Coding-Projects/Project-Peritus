//! Versioned bounded helper manifest and activation-record protocol.

use std::path::{Path, PathBuf};

use peritus_process::CommandSpec;
use peritus_sandbox::CheckedSandboxPlan;
use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    CompiledSeatbeltProfile, EnvironmentEntry, MacosError, MacosErrorKind, MacosOperation,
    ProcessContainment, ProxyHandleDescriptor, ProxyRoute, RecoveryAction, ResourceControlPlan,
    SecretHandleDescriptor, TerminalMapping, canonical::Writer, error,
};

mod codec;
mod fields;

use fields::{
    encode_containment, encode_proxy, encode_resources, encode_strings, encode_terminal,
    expected_preparation, path_text, validate_control_environment, validate_executable_path,
    validate_executable_text, validate_protected_handles, validate_working_directory,
};

const MAGIC: [u8; 8] = *b"PRTSMAC1";
const VERSION: u16 = 1;
const CHECKSUM_BYTES: usize = Sha256Digest::LENGTH;
const MAX_FRAME_BYTES: usize = 512 * 1_024;
const MAX_ARGUMENTS: usize = 4_096;
const PREPARATION_DOMAIN: &[u8] = b"PERITUS-SANDBOX-PREPARATION-V1\0";

/// The protected manifest frame arrives on helper standard input.
pub const MANIFEST_DESCRIPTOR_NUMBER: u32 = 0;
/// Human-readable protocol descriptor used in diagnostics.
pub const MANIFEST_DESCRIPTOR: &str = "protected inherited stdin frame";

/// Location of the protected manifest channel in the helper.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ManifestHandle {
    descriptor: u32,
}

impl ManifestHandle {
    /// Returns the fixed protected standard-input manifest channel.
    #[must_use]
    pub const fn protected_stdin() -> Self {
        Self { descriptor: MANIFEST_DESCRIPTOR_NUMBER }
    }

    /// Returns the helper descriptor number.
    #[must_use]
    pub const fn descriptor(self) -> u32 {
        self.descriptor
    }
}

/// Complete target and native-control data delivered to the helper as bounded binary bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelperManifest {
    process_id: ProcessId,
    plan_digest: Sha256Digest,
    descriptor_digest: Sha256Digest,
    support_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
    profile_digest: Sha256Digest,
    profile: String,
    seatbelt_executable: PathBuf,
    target_executable: String,
    target_arguments: Vec<String>,
    working_directory: PathBuf,
    environment: Vec<EnvironmentEntry>,
    exec_status_descriptor: u32,
    proxy: Option<ProxyHandleDescriptor>,
    resources: ResourceControlPlan,
    containment: ProcessContainment,
    terminal: TerminalMapping,
    secrets: Vec<SecretHandleDescriptor>,
    canonical: Vec<u8>,
    digest: Sha256Digest,
}

impl HelperManifest {
    /// Builds and checks a complete helper manifest.
    ///
    /// # Errors
    /// Rejects mismatched preparation identity, invalid executable paths, descriptor collisions,
    /// incomplete resource enforcement, or an over-limit representation.
    #[allow(clippy::too_many_arguments, reason = "one argument per closed manifest domain")]
    pub fn build(
        process_id: ProcessId,
        plan: &CheckedSandboxPlan,
        descriptor_digest: Sha256Digest,
        support_digest: Sha256Digest,
        preparation_digest: Sha256Digest,
        profile: &CompiledSeatbeltProfile,
        seatbelt_executable: PathBuf,
        command: &CommandSpec,
        working_directory: PathBuf,
        mut environment: Vec<EnvironmentEntry>,
        exec_status_descriptor: u32,
        proxy: Option<ProxyHandleDescriptor>,
        resources: ResourceControlPlan,
        containment: ProcessContainment,
        terminal: TerminalMapping,
        secrets: Vec<SecretHandleDescriptor>,
    ) -> Result<Self, MacosError> {
        if expected_preparation(plan.digest(), descriptor_digest, support_digest)
            != preparation_digest
        {
            return Err(error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "admitted preparation digest is not bound to this plan and descriptor",
            ));
        }
        if command.executable() != plan.requirements().process().program().as_str() {
            return Err(error::mismatch(
                MacosErrorKind::PreparationMismatch,
                "target executable differs from checked process requirements",
            ));
        }
        validate_executable_path(&seatbelt_executable)?;
        validate_executable_text(command.executable())?;
        validate_working_directory(&working_directory)?;
        crate::environment::canonicalize(&mut environment)?;
        validate_control_environment(&environment, proxy.as_ref(), &secrets)?;
        if !resources.is_complete() {
            return Err(MacosError::new(
                MacosErrorKind::UnsupportedHost,
                MacosOperation::Prepare,
                RecoveryAction::SelectSupportedBackend,
                "one or more required resource dimensions are unsupported",
            ));
        }
        validate_protected_handles(exec_status_descriptor, proxy.as_ref(), &secrets)?;
        let mut manifest = Self {
            process_id,
            plan_digest: plan.digest(),
            descriptor_digest,
            support_digest,
            preparation_digest,
            profile_digest: profile.digest(),
            profile: profile.text().to_owned(),
            seatbelt_executable,
            target_executable: command.executable().to_owned(),
            target_arguments: command.arguments().to_vec(),
            working_directory,
            environment,
            exec_status_descriptor,
            proxy,
            resources,
            containment,
            terminal,
            secrets,
            canonical: Vec::new(),
            digest: Sha256Digest::new([0; 32]),
        };
        manifest.canonical = manifest.encode()?;
        manifest.digest = peritus_codec::sha256(&manifest.canonical);
        Ok(manifest)
    }

    /// Returns exact canonical manifest bytes, including its internal checksum.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }

    /// Returns the digest of exact canonical manifest bytes.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Returns the exact process owner identity.
    #[must_use]
    pub const fn process_id(&self) -> ProcessId {
        self.process_id
    }

    /// Returns the checked sandbox-plan digest.
    #[must_use]
    pub const fn plan_digest(&self) -> Sha256Digest {
        self.plan_digest
    }

    /// Returns the admitted descriptor digest.
    #[must_use]
    pub const fn descriptor_digest(&self) -> Sha256Digest {
        self.descriptor_digest
    }

    /// Returns the admitted support digest.
    #[must_use]
    pub const fn support_digest(&self) -> Sha256Digest {
        self.support_digest
    }

    /// Returns the admitted preparation digest.
    #[must_use]
    pub const fn preparation_digest(&self) -> Sha256Digest {
        self.preparation_digest
    }

    /// Returns the exact Seatbelt profile digest.
    #[must_use]
    pub const fn profile_digest(&self) -> Sha256Digest {
        self.profile_digest
    }

    /// Returns exact Seatbelt profile text.
    #[must_use]
    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// Returns the checked Seatbelt executable path.
    #[must_use]
    pub fn seatbelt_executable(&self) -> &Path {
        &self.seatbelt_executable
    }

    /// Returns the literal target executable.
    #[must_use]
    pub fn target_executable(&self) -> &str {
        &self.target_executable
    }

    /// Returns literal target argv excluding argv zero.
    #[must_use]
    pub fn target_arguments(&self) -> &[String] {
        &self.target_arguments
    }

    /// Returns the exact canonical target working directory.
    #[must_use]
    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    /// Returns the complete exact non-secret target environment.
    #[must_use]
    pub fn environment(&self) -> &[EnvironmentEntry] {
        &self.environment
    }

    /// Returns the inherited close-on-exec helper status descriptor.
    #[must_use]
    pub const fn exec_status_descriptor(&self) -> u32 {
        self.exec_status_descriptor
    }

    /// Returns the managed proxy route, if egress is permitted.
    #[must_use]
    pub const fn proxy(&self) -> Option<ProxyRoute> {
        match &self.proxy {
            Some(proxy) => Some(proxy.route()),
            None => None,
        }
    }

    /// Returns exact nonsensitive proxy protected-handle metadata.
    #[must_use]
    pub const fn proxy_descriptor(&self) -> Option<&ProxyHandleDescriptor> {
        self.proxy.as_ref()
    }

    /// Returns all dimension-specific resource controls.
    #[must_use]
    pub const fn resources(&self) -> &ResourceControlPlan {
        &self.resources
    }

    /// Returns complete process containment requirements.
    #[must_use]
    pub const fn containment(&self) -> ProcessContainment {
        self.containment
    }

    /// Returns exact pipe or PTY mapping.
    #[must_use]
    pub const fn terminal(&self) -> TerminalMapping {
        self.terminal
    }

    /// Returns secret descriptor numbers only; secret bytes are never manifest data.
    #[must_use]
    pub fn secrets(&self) -> &[SecretHandleDescriptor] {
        &self.secrets
    }

    fn encode(&self) -> Result<Vec<u8>, MacosError> {
        let mut body = Writer::new();
        body.fixed(self.process_id.as_bytes())?;
        body.fixed(self.plan_digest.as_bytes())?;
        body.fixed(self.descriptor_digest.as_bytes())?;
        body.fixed(self.support_digest.as_bytes())?;
        body.fixed(self.preparation_digest.as_bytes())?;
        body.fixed(self.profile_digest.as_bytes())?;
        body.string(&self.profile)?;
        body.string(path_text(&self.seatbelt_executable)?)?;
        body.string(&self.target_executable)?;
        encode_strings(&mut body, &self.target_arguments)?;
        body.string(path_text(&self.working_directory)?)?;
        body.count(self.environment.len())?;
        for entry in &self.environment {
            entry.encode(&mut body)?;
        }
        body.u32(self.exec_status_descriptor)?;
        encode_proxy(&mut body, self.proxy.as_ref())?;
        encode_resources(&mut body, &self.resources)?;
        encode_containment(&mut body, self.containment)?;
        encode_terminal(&mut body, self.terminal)?;
        body.count(self.secrets.len())?;
        for secret in &self.secrets {
            secret.encode(&mut body)?;
        }
        let body = body.finish();
        let mut envelope = Writer::new();
        envelope.fixed(&MAGIC)?;
        envelope.u16(VERSION)?;
        envelope.u32(u32::try_from(body.len()).map_err(|_| {
            error::limited(MacosOperation::Manifest, "manifest body is too large")
        })?)?;
        envelope.bytes(&body)?;
        let mut bytes = envelope.finish();
        let checksum = peritus_codec::sha256(&bytes);
        bytes.extend_from_slice(checksum.as_bytes());
        if bytes.len() > MAX_FRAME_BYTES {
            return Err(error::limited(MacosOperation::Manifest, "manifest exceeds frame bound"));
        }
        Ok(bytes)
    }
}

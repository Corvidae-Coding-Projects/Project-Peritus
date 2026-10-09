//! Version-one bounded Windows helper manifest.

mod codec;

use std::io::Read;

use peritus_process::CommandSpec;
use peritus_sandbox::{BackendAdmission, CheckedSandboxPlan};
use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    AclPlan, InheritedHandlePolicy, JobPlan, NetworkIsolation, ProcessPolicy,
    ProtectedSecretHandle, ResourceControlPlan, TerminalMapping, TokenProfile, WindowsError,
    WindowsErrorKind, WindowsOperation, WindowsPath, WindowsRecovery, error,
};

const MAX_MANIFEST_BYTES: usize = 4 * 1_024 * 1_024;
const MAX_ARGUMENTS: usize = 4_096;
const MAX_ENVIRONMENT: usize = 4_096;

/// One ordinary environment value copied from the exact C2 execution plan.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct EnvironmentEntry {
    name: String,
    value: String,
}

impl EnvironmentEntry {
    /// Creates a bounded Windows environment entry.
    ///
    /// # Errors
    /// Rejects empty/invalid names, NUL values, or over-limit text.
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Result<Self, WindowsError> {
        let name = name.into();
        let value = value.into();
        if name.is_empty()
            || name.len() > 32_767
            || value.len() > 1_048_576
            || name.contains(['=', '\0'])
            || value.contains('\0')
        {
            return Err(error::invalid(
                WindowsOperation::Manifest,
                "environment entry is invalid or exceeds its bound",
            ));
        }
        Ok(Self { name, value })
    }

    /// Returns the exact case-preserving environment name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the ordinary non-secret value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Complete immutable data accepted by the Windows helper.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelperManifest {
    process_id: ProcessId,
    plan_digest: Sha256Digest,
    descriptor_digest: Sha256Digest,
    support_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
    helper_digest: Sha256Digest,
    acl_digest: Sha256Digest,
    token: TokenProfile,
    executable: String,
    arguments: Vec<String>,
    working_directory: WindowsPath,
    environment: Vec<EnvironmentEntry>,
    job: JobPlan,
    process: ProcessPolicy,
    terminal: TerminalMapping,
    resources: ResourceControlPlan,
    network: NetworkIsolation,
    secret_handles: Vec<ProtectedSecretHandle>,
    inherited_handles: InheritedHandlePolicy,
    canonical: Vec<u8>,
    digest: Sha256Digest,
}

impl HelperManifest {
    /// Builds and checks one complete helper manifest.
    ///
    /// # Errors
    /// Rejects any preparation drift, root-command drift, incomplete resource mapping, handle
    /// mismatch, collection bound, or noncanonical environment.
    #[allow(clippy::too_many_arguments, reason = "one argument per closed native domain")]
    pub fn build(
        process_id: ProcessId,
        sandbox: &CheckedSandboxPlan,
        admission: &BackendAdmission,
        helper_digest: Sha256Digest,
        acl: &AclPlan,
        token: TokenProfile,
        command: &CommandSpec,
        working_directory: WindowsPath,
        mut environment: Vec<EnvironmentEntry>,
        job: JobPlan,
        process: ProcessPolicy,
        terminal: TerminalMapping,
        resources: ResourceControlPlan,
        network: NetworkIsolation,
        secret_handles: Vec<ProtectedSecretHandle>,
        inherited_handles: InheritedHandlePolicy,
    ) -> Result<Self, WindowsError> {
        if expected_preparation(
            sandbox.digest(),
            admission.descriptor_digest(),
            admission.support_digest(),
        ) != admission.preparation_digest()
        {
            return Err(binding_error("admitted preparation digest is not bound to native facts"));
        }
        if command.executable() != sandbox.requirements().process().program().as_str() {
            return Err(binding_error("literal target executable differs from checked process"));
        }
        if command.arguments().len() > MAX_ARGUMENTS || environment.len() > MAX_ENVIRONMENT {
            return Err(error::invalid(
                WindowsOperation::Manifest,
                "target arguments or environment exceed manifest bounds",
            ));
        }
        if !resources.is_complete() {
            return Err(error::unsupported(
                WindowsOperation::Prepare,
                "one or more resource dimensions have no enforcement owner",
            ));
        }
        environment.sort();
        for pair in environment.windows(2) {
            if pair[0].name.eq_ignore_ascii_case(&pair[1].name) {
                return Err(error::invalid(
                    WindowsOperation::Manifest,
                    "environment contains a case-fold name alias",
                ));
            }
        }
        validate_handle_binding(&secret_handles, &inherited_handles)?;
        let executable = WindowsPath::from_sandbox(
            &working_directory,
            sandbox.requirements().process().program(),
        )?
        .as_str()
        .to_owned();
        let mut manifest = Self {
            process_id,
            plan_digest: sandbox.digest(),
            descriptor_digest: admission.descriptor_digest(),
            support_digest: admission.support_digest(),
            preparation_digest: admission.preparation_digest(),
            helper_digest,
            acl_digest: acl.digest(),
            token,
            executable,
            arguments: command.arguments().to_vec(),
            working_directory,
            environment,
            job,
            process,
            terminal,
            resources,
            network,
            secret_handles,
            inherited_handles,
            canonical: Vec::new(),
            digest: Sha256Digest::new([0; 32]),
        };
        manifest.canonical = codec::encode(&manifest)?;
        manifest.digest = peritus_codec::sha256(&manifest.canonical);
        Ok(manifest)
    }

    /// Decodes, checksums, bounds, and canonicalizes helper bytes.
    ///
    /// # Errors
    /// Rejects malformed, unsupported, mismatched, or noncanonical bytes.
    pub fn decode(bytes: &[u8]) -> Result<Self, WindowsError> {
        codec::decode(bytes)
    }

    /// Reads C2's little-endian length-prefixed protected stdin frame.
    ///
    /// # Errors
    /// Rejects I/O failure, zero/excessive length, or an invalid manifest.
    pub fn read_framed(mut reader: impl Read) -> Result<Self, WindowsError> {
        let mut length = [0_u8; 4];
        reader
            .read_exact(&mut length)
            .map_err(|_| error::io(WindowsOperation::Manifest, "manifest length cannot be read"))?;
        let length = usize::try_from(u32::from_le_bytes(length)).map_err(|_| {
            error::invalid(WindowsOperation::Manifest, "manifest frame length overflowed")
        })?;
        if length == 0 || length > MAX_MANIFEST_BYTES {
            return Err(error::invalid(
                WindowsOperation::Manifest,
                "manifest frame is empty or exceeds its bound",
            ));
        }
        let mut bytes = vec![0_u8; length];
        reader
            .read_exact(&mut bytes)
            .map_err(|_| error::io(WindowsOperation::Manifest, "manifest frame is truncated"))?;
        Self::decode(&bytes)
    }

    /// Returns process identity.
    #[must_use]
    pub const fn process_id(&self) -> ProcessId {
        self.process_id
    }
    /// Returns checked sandbox digest.
    #[must_use]
    pub const fn plan_digest(&self) -> Sha256Digest {
        self.plan_digest
    }
    /// Returns descriptor digest.
    #[must_use]
    pub const fn descriptor_digest(&self) -> Sha256Digest {
        self.descriptor_digest
    }
    /// Returns support digest.
    #[must_use]
    pub const fn support_digest(&self) -> Sha256Digest {
        self.support_digest
    }
    /// Returns admitted preparation digest.
    #[must_use]
    pub const fn preparation_digest(&self) -> Sha256Digest {
        self.preparation_digest
    }
    /// Returns helper identity digest.
    #[must_use]
    pub const fn helper_digest(&self) -> Sha256Digest {
        self.helper_digest
    }
    /// Returns exact ACL-plan digest.
    #[must_use]
    pub const fn acl_digest(&self) -> Sha256Digest {
        self.acl_digest
    }
    /// Returns token/AppContainer plan.
    #[must_use]
    pub const fn token(&self) -> &TokenProfile {
        &self.token
    }
    /// Returns literal target executable.
    #[must_use]
    pub fn executable(&self) -> &str {
        &self.executable
    }
    /// Returns literal target argv excluding argv zero.
    #[must_use]
    pub fn arguments(&self) -> &[String] {
        &self.arguments
    }
    /// Returns exact working directory.
    #[must_use]
    pub const fn working_directory(&self) -> &WindowsPath {
        &self.working_directory
    }
    /// Returns ordinary non-secret environment.
    #[must_use]
    pub fn environment(&self) -> &[EnvironmentEntry] {
        &self.environment
    }
    /// Returns Job Object policy.
    #[must_use]
    pub const fn job(&self) -> JobPlan {
        self.job
    }
    /// Returns process-control policy.
    #[must_use]
    pub const fn process(&self) -> ProcessPolicy {
        self.process
    }
    /// Returns terminal mapping.
    #[must_use]
    pub const fn terminal(&self) -> TerminalMapping {
        self.terminal
    }
    /// Returns all resource mappings.
    #[must_use]
    pub const fn resources(&self) -> ResourceControlPlan {
        self.resources
    }
    /// Returns fail-closed network selection.
    #[must_use]
    pub const fn network(&self) -> NetworkIsolation {
        self.network
    }
    /// Returns nonsensitive secret handle descriptors.
    #[must_use]
    pub fn secret_handles(&self) -> &[ProtectedSecretHandle] {
        &self.secret_handles
    }
    /// Returns exact inherited-handle whitelist.
    #[must_use]
    pub const fn inherited_handles(&self) -> &InheritedHandlePolicy {
        &self.inherited_handles
    }
    /// Returns canonical bytes including checksum.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }
    /// Returns complete manifest digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn validate_handle_binding(
    secrets: &[ProtectedSecretHandle],
    inherited: &InheritedHandlePolicy,
) -> Result<(), WindowsError> {
    let required = secrets
        .iter()
        .filter(|handle| {
            matches!(handle.destination(), crate::SecretHandleDestination::Brokered(_))
        })
        .map(ProtectedSecretHandle::handle);
    if required.into_iter().any(|handle| !inherited.handles().contains(&handle)) {
        return Err(binding_error("protected route/secret handle is absent from whitelist"));
    }
    Ok(())
}

pub(crate) fn expected_preparation(
    plan: Sha256Digest,
    descriptor: Sha256Digest,
    support: Sha256Digest,
) -> Sha256Digest {
    let mut bytes = Vec::from(b"PERITUS-SANDBOX-PREPARATION-V1\0".as_slice());
    bytes.extend_from_slice(plan.as_bytes());
    bytes.extend_from_slice(descriptor.as_bytes());
    bytes.extend_from_slice(support.as_bytes());
    peritus_codec::sha256(&bytes)
}

fn binding_error(detail: &'static str) -> WindowsError {
    WindowsError::new(
        WindowsErrorKind::PreparationMismatch,
        WindowsOperation::Manifest,
        WindowsRecovery::Replan,
        detail,
    )
}

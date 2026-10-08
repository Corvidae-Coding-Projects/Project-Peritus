//! Versioned exact-native Windows helper manifest.

mod codec;

use std::{
    ffi::{OsStr, OsString},
    io::{ErrorKind, Read},
};

use peritus_process::CommandSpec;
use peritus_sandbox::{BackendAdmission, CheckedSandboxPlan};
use peritus_types::{ProcessId, Sha256Digest};

use crate::{
    AclPlan, InheritedHandlePolicy, JobPlan, NetworkIsolation, ProcessPolicy,
    ProtectedSecretHandle, ResourceControlPlan, TerminalMapping, TokenProfile, WindowsError,
    WindowsErrorKind, WindowsOperation, WindowsPath, WindowsRecovery, error,
};

/// One ordinary environment value copied from the exact C2 execution plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EnvironmentEntry {
    name: OsString,
    value: OsString,
}

impl EnvironmentEntry {
    /// Creates a Windows environment entry representable by the protected native wire format.
    ///
    /// # Errors
    /// Rejects empty/invalid names or NUL values.
    pub fn new(
        name: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> Result<Self, WindowsError> {
        let name = name.into();
        let value = value.into();
        let name_units = wide_units(&name);
        let value_units = wide_units(&value);
        if name.is_empty()
            || name_units.iter().any(|unit| matches!(*unit, 0 | 61))
            || value_units.contains(&0)
        {
            return Err(error::invalid(
                WindowsOperation::Manifest,
                "environment entry has an empty name or contains '=' or NUL",
            ));
        }
        Ok(Self { name, value })
    }

    pub(super) fn new_legacy(name: String, value: String) -> Result<Self, WindowsError> {
        if name.is_empty()
            || name.contains(['=', '\0'])
            || value.contains('\0')
        {
            return Err(error::invalid(
                WindowsOperation::Manifest,
                "environment entry has an empty name or contains '=' or NUL",
            ));
        }
        Ok(Self { name: name.into(), value: value.into() })
    }

    /// Returns the exact case-preserving environment name.
    #[must_use]
    pub fn name(&self) -> &OsStr {
        &self.name
    }

    /// Returns the ordinary non-secret value.
    #[must_use]
    pub fn value(&self) -> &OsStr {
        &self.value
    }
}

#[cfg(windows)]
fn wide_units(value: &OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    value.encode_wide().collect()
}

#[cfg(not(windows))]
fn wide_units(value: &OsStr) -> Vec<u16> {
    value.to_string_lossy().encode_utf16().collect()
}

#[cfg(windows)]
pub(super) fn windows_name_cmp(left: &OsStr, right: &OsStr) -> std::cmp::Ordering {
    peritus_process::native_environment_name_cmp(left, right)
}

#[cfg(not(windows))]
pub(super) fn windows_name_cmp(left: &OsStr, right: &OsStr) -> std::cmp::Ordering {
    windows_ordinal_key(left).cmp(&windows_ordinal_key(right))
}

#[cfg(not(windows))]
fn windows_ordinal_key(value: &OsStr) -> Vec<u16> {
    let mut key = Vec::new();
    for character in value.to_string_lossy().chars() {
        for folded in character.to_uppercase() {
            let mut encoded = [0_u16; 2];
            key.extend_from_slice(folded.encode_utf16(&mut encoded));
        }
    }
    key
}

/// Complete immutable data accepted by the Windows helper.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelperManifest {
    encoding_version: u16,
    process_id: ProcessId,
    plan_digest: Sha256Digest,
    descriptor_digest: Sha256Digest,
    support_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
    helper_digest: Sha256Digest,
    acl_digest: Sha256Digest,
    token: TokenProfile,
    executable: OsString,
    arguments: Vec<OsString>,
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
    canonical_page_ends: Vec<usize>,
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
        let authorized = sandbox.native_execution().map_or_else(
            || OsStr::new(sandbox.requirements().process().program().as_str()),
            peritus_sandbox::NativeExecutionAuthority::executable,
        );
        if command.executable() != authorized {
            return Err(binding_error("literal target executable differs from checked process"));
        }
        if !resources.is_complete() {
            return Err(error::unsupported(
                WindowsOperation::Prepare,
                "one or more resource dimensions have no enforcement owner",
            ));
        }
        environment.sort_by(|left, right| windows_name_cmp(left.name(), right.name()));
        for pair in environment.windows(2) {
            if windows_name_cmp(pair[0].name(), pair[1].name()).is_eq() {
                return Err(error::invalid(
                    WindowsOperation::Manifest,
                    "environment contains a case-fold name alias",
                ));
            }
        }
        validate_handle_binding(&secret_handles, &inherited_handles)?;
        let mut manifest = Self {
            encoding_version: codec::SCHEMA,
            process_id,
            plan_digest: sandbox.digest(),
            descriptor_digest: admission.descriptor_digest(),
            support_digest: admission.support_digest(),
            preparation_digest: admission.preparation_digest(),
            helper_digest,
            acl_digest: acl.digest(),
            token,
            executable: command.executable().to_owned(),
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
            canonical_page_ends: Vec::new(),
            digest: Sha256Digest::new([0; 32]),
        };
        manifest.canonical = codec::encode(&manifest)?;
        manifest.canonical_page_ends = codec::page_ends(&manifest.canonical)?;
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
        Self::read_framed_while(&mut reader, &mut || true)
    }

    /// Reads a single-frame or paged protected manifest while owner continuation is retained.
    ///
    /// # Errors
    /// Rejects cancellation, I/O failure, an empty/oversized physical page, allocation failure, or
    /// an invalid manifest.
    pub fn read_framed_while(
        mut reader: impl Read,
        should_continue: &mut dyn FnMut() -> bool,
    ) -> Result<Self, WindowsError> {
        let first = read_frame_length(&mut reader, should_continue)?;
        if first != peritus_process::NATIVE_MANIFEST_STREAM_MARKER {
            return Self::decode(&read_physical_frame(&mut reader, first, should_continue)?);
        }
        let mut bytes = Vec::new();
        loop {
            let length = read_frame_length(&mut reader, should_continue)?;
            if length == 0 {
                break;
            }
            let page = read_physical_frame(&mut reader, length, should_continue)?;
            bytes.try_reserve(page.len()).map_err(|_| {
                WindowsError::new(
                    WindowsErrorKind::HelperProtocol,
                    WindowsOperation::Manifest,
                    WindowsRecovery::RepairHelper,
                    "manifest page-stream allocation is unavailable",
                )
            })?;
            bytes.extend_from_slice(&page);
        }
        if bytes.is_empty() {
            return Err(error::invalid(
                WindowsOperation::Manifest,
                "manifest page stream is empty",
            ));
        }
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
    pub fn executable(&self) -> &OsStr {
        &self.executable
    }
    /// Returns literal target argv excluding argv zero.
    #[must_use]
    pub fn arguments(&self) -> &[OsString] {
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
    pub(crate) fn canonical_pages(&self) -> impl ExactSizeIterator<Item = &[u8]> {
        let mut start = 0_usize;
        self.canonical_page_ends.iter().map(move |end| {
            let page = &self.canonical[start..*end];
            start = *end;
            page
        })
    }
    /// Returns complete manifest digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }
}

fn read_frame_length(
    reader: &mut impl Read,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<u32, WindowsError> {
    let mut length = [0_u8; 4];
    read_exact_while(reader, &mut length, should_continue, "manifest length cannot be read")?;
    Ok(u32::from_le_bytes(length))
}

fn read_physical_frame(
    reader: &mut impl Read,
    length: u32,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<Vec<u8>, WindowsError> {
    let length = usize::try_from(length).map_err(|_| {
        error::invalid(WindowsOperation::Manifest, "manifest frame length is not representable")
    })?;
    if length == 0 || length > peritus_process::NATIVE_MANIFEST_FRAME_BYTES {
        return Err(error::invalid(
            WindowsOperation::Manifest,
            "manifest physical frame is empty or exceeds transport capacity",
        ));
    }
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(length).map_err(|_| {
        WindowsError::new(
            WindowsErrorKind::HelperProtocol,
            WindowsOperation::Manifest,
            WindowsRecovery::RepairHelper,
            "manifest physical-frame allocation is unavailable",
        )
    })?;
    bytes.resize(length, 0);
    read_exact_while(reader, &mut bytes, should_continue, "manifest frame is truncated")?;
    Ok(bytes)
}

fn read_exact_while(
    reader: &mut impl Read,
    bytes: &mut [u8],
    should_continue: &mut dyn FnMut() -> bool,
    failure: &'static str,
) -> Result<(), WindowsError> {
    let mut offset = 0_usize;
    while offset < bytes.len() {
        if !should_continue() {
            return Err(WindowsError::new(
                WindowsErrorKind::HelperProtocol,
                WindowsOperation::Manifest,
                WindowsRecovery::CancelAndReap,
                "manifest read was cancelled by its retained owner",
            ));
        }
        match reader.read(&mut bytes[offset..]) {
            Ok(0) => return Err(error::io(WindowsOperation::Manifest, failure)),
            Ok(count) => offset += count,
            Err(source) if source.kind() == ErrorKind::Interrupted => {}
            Err(source) if source.kind() == ErrorKind::WouldBlock => std::thread::yield_now(),
            Err(_) => return Err(error::io(WindowsOperation::Manifest, failure)),
        }
    }
    Ok(())
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

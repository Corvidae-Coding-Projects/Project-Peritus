//! Versioned helper manifest with exact native command and environment values.

mod activation;
mod delivery;
mod value;

pub use activation::ActivationRecord;
pub use value::{EnvironmentEntry, InheritedHandle, ProtectedPayloadBinding, TargetCommand};

use crate::{
    LandlockAccess, LandlockRule, LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery,
    NetworkIsolation, ResourcePlan,
};
use peritus_sandbox::{SecretGrant, SecretReference};
use peritus_types::ResourceId;
use peritus_types::Sha256Digest;
use std::ffi::OsString;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;

const MANIFEST_MAGIC: [u8; 8] = *b"PRTLNXM1";
const LEGACY_VERSION: u16 = 1;
const VERSION: u16 = 2;

/// Complete versioned helper preparation input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HelperManifest {
    encoding_version: u16,
    plan_digest: Sha256Digest,
    backend_digest: Sha256Digest,
    support_digest: Sha256Digest,
    preparation_digest: Sha256Digest,
    target: TargetCommand,
    working_directory: PathBuf,
    cgroup_leaf: PathBuf,
    pty: bool,
    environment: Vec<EnvironmentEntry>,
    landlock_rules: Vec<LandlockRule>,
    resources: ResourcePlan,
    network: NetworkIsolation,
    inherited_handles: Vec<InheritedHandle>,
    protected_payloads: Vec<ProtectedPayloadBinding>,
}

impl HelperManifest {
    /// Creates and validates a complete helper manifest.
    ///
    /// # Errors
    /// Rejects relative working directories, duplicate names/handles, and invalid control data.
    #[allow(clippy::too_many_arguments, reason = "complete authority-relevant helper manifest")]
    pub fn new(
        plan_digest: Sha256Digest,
        backend_digest: Sha256Digest,
        support_digest: Sha256Digest,
        preparation_digest: Sha256Digest,
        target: TargetCommand,
        working_directory: PathBuf,
        cgroup_leaf: PathBuf,
        pty: bool,
        mut environment: Vec<EnvironmentEntry>,
        mut landlock_rules: Vec<LandlockRule>,
        resources: ResourcePlan,
        network: NetworkIsolation,
        mut inherited_handles: Vec<InheritedHandle>,
    ) -> Result<Self, LinuxError> {
        if !working_directory.is_absolute()
            || !valid_cgroup_leaf(&cgroup_leaf)
            || landlock_rules.len() > 256
            || inherited_handles.len() > 256
        {
            return Err(manifest_error("manifest path or collection is invalid"));
        }
        environment.sort_by(|left, right| left.name.as_bytes().cmp(right.name.as_bytes()));
        if environment
            .windows(2)
            .any(|pair| pair[0].name.as_bytes() == pair[1].name.as_bytes())
        {
            return Err(manifest_error("manifest environment contains duplicate names"));
        }
        landlock_rules.sort();
        landlock_rules.dedup();
        inherited_handles.sort();
        if inherited_handles
            .windows(2)
            .any(|pair| pair[0].descriptor == pair[1].descriptor || pair[0].label == pair[1].label)
        {
            return Err(manifest_error("manifest inherited handles collide"));
        }
        Ok(Self {
            encoding_version: VERSION,
            plan_digest,
            backend_digest,
            support_digest,
            preparation_digest,
            target,
            working_directory,
            cgroup_leaf,
            pty,
            environment,
            landlock_rules,
            resources,
            network,
            inherited_handles,
            protected_payloads: Vec::new(),
        })
    }

    /// Adds exact checked protected-payload bindings.
    ///
    /// # Errors
    /// Rejects duplicate checked requirements, delivery destinations, descriptors, or labels.
    pub fn with_protected_payloads(
        mut self,
        mut protected_payloads: Vec<ProtectedPayloadBinding>,
    ) -> Result<Self, LinuxError> {
        if protected_payloads.len() > 128 {
            return Err(manifest_error("protected payload count exceeds its bound"));
        }
        protected_payloads.sort();
        for (index, binding) in protected_payloads.iter().enumerate() {
            if self.inherited_handles.iter().any(|handle| {
                handle.descriptor == binding.handle.descriptor
                    || handle.label == binding.handle.label
            }) || protected_payloads[..index].iter().any(|prior| {
                prior.requirement == binding.requirement
                    || prior.requirement.delivery() == binding.requirement.delivery()
                    || prior.handle.descriptor == binding.handle.descriptor
                    || prior.handle.label == binding.handle.label
            }) {
                return Err(manifest_error(
                    "protected payload requirement, destination, descriptor, or label collides",
                ));
            }
        }
        self.protected_payloads = protected_payloads;
        Ok(self)
    }
    /// Returns the checked sandbox plan digest.
    #[must_use]
    pub const fn plan_digest(&self) -> Sha256Digest {
        self.plan_digest
    }
    /// Returns the selected backend digest.
    #[must_use]
    pub const fn backend_digest(&self) -> Sha256Digest {
        self.backend_digest
    }
    /// Returns the support digest.
    #[must_use]
    pub const fn support_digest(&self) -> Sha256Digest {
        self.support_digest
    }
    /// Returns the admitted preparation digest.
    #[must_use]
    pub const fn preparation_digest(&self) -> Sha256Digest {
        self.preparation_digest
    }
    /// Returns the literal target command.
    #[must_use]
    pub const fn target(&self) -> &TargetCommand {
        &self.target
    }
    /// Returns the exact working directory.
    #[must_use]
    pub fn working_directory(&self) -> &std::path::Path {
        &self.working_directory
    }
    /// Returns the exact prepared cgroup leaf used for pre-activation self-attachment.
    #[must_use]
    pub fn cgroup_leaf(&self) -> &std::path::Path {
        &self.cgroup_leaf
    }
    /// Reports whether C2 must supply a process-owned PTY slave attachment.
    #[must_use]
    pub const fn expects_pty(&self) -> bool {
        self.pty
    }
    /// Returns exact non-secret environment entries.
    #[must_use]
    pub fn environment(&self) -> &[EnvironmentEntry] {
        &self.environment
    }
    /// Returns filesystem rules to install.
    #[must_use]
    pub fn landlock_rules(&self) -> &[LandlockRule] {
        &self.landlock_rules
    }
    /// Returns resource limits.
    #[must_use]
    pub const fn resources(&self) -> ResourcePlan {
        self.resources
    }
    /// Returns network isolation.
    #[must_use]
    pub const fn network(&self) -> NetworkIsolation {
        self.network
    }
    /// Returns exact protected descriptors retained for the target.
    #[must_use]
    pub fn inherited_handles(&self) -> &[InheritedHandle] {
        &self.inherited_handles
    }
    /// Returns exact checked protected-payload bindings.
    #[must_use]
    pub fn protected_payloads(&self) -> &[ProtectedPayloadBinding] {
        &self.protected_payloads
    }

    /// Encodes with a trailing SHA-256 checksum over every preceding byte.
    ///
    /// # Errors
    /// Returns a helper protocol error if exact native values exceed the protocol's u32 framing.
    pub fn encode(&self) -> Result<Vec<u8>, LinuxError> {
        if self.encoding_version == LEGACY_VERSION {
            return self.encode_legacy();
        }
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MANIFEST_MAGIC);
        bytes.extend_from_slice(&VERSION.to_be_bytes());
        for digest in
            [self.plan_digest, self.backend_digest, self.support_digest, self.preparation_digest]
        {
            bytes.extend_from_slice(digest.as_bytes());
        }
        crate::canonical::push_bytes_unbounded(&mut bytes, self.target.program().as_bytes())?;
        crate::canonical::push_count_unbounded(&mut bytes, self.target.arguments().len())?;
        for argument in self.target.arguments() {
            crate::canonical::push_bytes_unbounded(&mut bytes, argument.as_bytes())?;
        }
        crate::canonical::push_bytes_unbounded(
            &mut bytes,
            self.working_directory.as_os_str().as_bytes(),
        )?;
        crate::canonical::push_bytes_unbounded(&mut bytes, self.cgroup_leaf.as_os_str().as_bytes())?;
        bytes.push(u8::from(self.pty));
        crate::canonical::push_count_unbounded(&mut bytes, self.environment.len())?;
        for entry in &self.environment {
            crate::canonical::push_bytes_unbounded(&mut bytes, entry.name().as_bytes())?;
            crate::canonical::push_bytes_unbounded(&mut bytes, entry.value().as_bytes())?;
        }
        crate::canonical::push_bounded_count_unbounded_total(
            &mut bytes,
            self.landlock_rules.len(),
        )?;
        for rule in &self.landlock_rules {
            crate::canonical::push_bytes_unbounded(&mut bytes, rule.path().as_os_str().as_bytes())?;
            bytes.extend_from_slice(&rule.access().bits().to_be_bytes());
        }
        self.resources.encode(&mut bytes);
        bytes.push(self.network.tag());
        crate::canonical::push_bounded_count_unbounded_total(
            &mut bytes,
            self.inherited_handles.len(),
        )?;
        for handle in &self.inherited_handles {
            bytes.extend_from_slice(&handle.descriptor().to_be_bytes());
            crate::canonical::push_bytes_unbounded(&mut bytes, handle.label().as_bytes())?;
        }
        crate::canonical::push_bounded_count_unbounded_total(
            &mut bytes,
            self.protected_payloads.len(),
        )?;
        for binding in &self.protected_payloads {
            bytes.extend_from_slice(binding.requirement.reference().resource_id().as_bytes());
            bytes.extend_from_slice(binding.requirement.reference().version().as_bytes());
            delivery::encode_v2(&mut bytes, binding.requirement.delivery())?;
            bytes.extend_from_slice(&binding.handle.descriptor().to_be_bytes());
            crate::canonical::push_bytes_unbounded(
                &mut bytes,
                binding.handle.label().as_bytes(),
            )?;
            bytes.extend_from_slice(&binding.payload_len.to_be_bytes());
        }
        let checksum = peritus_codec::sha256(&bytes);
        bytes.extend_from_slice(checksum.as_bytes());
        Ok(bytes)
    }

    fn encode_legacy(&self) -> Result<Vec<u8>, LinuxError> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&MANIFEST_MAGIC);
        bytes.extend_from_slice(&LEGACY_VERSION.to_be_bytes());
        for digest in
            [self.plan_digest, self.backend_digest, self.support_digest, self.preparation_digest]
        {
            bytes.extend_from_slice(digest.as_bytes());
        }
        crate::canonical::push_str(
            &mut bytes,
            self.target
                .program()
                .to_str()
                .ok_or_else(|| manifest_error("legacy target is not UTF-8"))?,
        )?;
        crate::canonical::push_count(&mut bytes, self.target.arguments().len())?;
        for argument in self.target.arguments() {
            crate::canonical::push_str(
                &mut bytes,
                argument
                    .to_str()
                    .ok_or_else(|| manifest_error("legacy argument is not UTF-8"))?,
            )?;
        }
        crate::canonical::push_str(
            &mut bytes,
            self.working_directory
                .to_str()
                .ok_or_else(|| manifest_error("legacy working directory is not UTF-8"))?,
        )?;
        crate::canonical::push_str(
            &mut bytes,
            self.cgroup_leaf
                .to_str()
                .ok_or_else(|| manifest_error("legacy cgroup path is not UTF-8"))?,
        )?;
        bytes.push(u8::from(self.pty));
        crate::canonical::push_count(&mut bytes, self.environment.len())?;
        for entry in &self.environment {
            crate::canonical::push_str(
                &mut bytes,
                entry.name().to_str().ok_or_else(|| manifest_error("legacy name is not UTF-8"))?,
            )?;
            crate::canonical::push_str(
                &mut bytes,
                entry.value().to_str().ok_or_else(|| manifest_error("legacy value is not UTF-8"))?,
            )?;
        }
        crate::canonical::push_count(&mut bytes, self.landlock_rules.len())?;
        for rule in &self.landlock_rules {
            crate::canonical::push_str(
                &mut bytes,
                rule.path().to_str().ok_or_else(|| manifest_error("legacy rule is not UTF-8"))?,
            )?;
            bytes.extend_from_slice(&rule.access().bits().to_be_bytes());
        }
        self.resources.encode(&mut bytes);
        bytes.push(self.network.tag());
        crate::canonical::push_count(&mut bytes, self.inherited_handles.len())?;
        for handle in &self.inherited_handles {
            bytes.extend_from_slice(&handle.descriptor().to_be_bytes());
            crate::canonical::push_str(&mut bytes, handle.label())?;
        }
        crate::canonical::push_count(&mut bytes, self.protected_payloads.len())?;
        for binding in &self.protected_payloads {
            bytes.extend_from_slice(binding.requirement.reference().resource_id().as_bytes());
            bytes.extend_from_slice(binding.requirement.reference().version().as_bytes());
            delivery::encode_legacy(&mut bytes, binding.requirement.delivery())?;
            bytes.extend_from_slice(&binding.handle.descriptor().to_be_bytes());
            crate::canonical::push_str(&mut bytes, binding.handle.label())?;
            bytes.extend_from_slice(&binding.payload_len.to_be_bytes());
        }
        crate::canonical::check_total(&bytes)?;
        let checksum = peritus_codec::sha256(&bytes);
        bytes.extend_from_slice(checksum.as_bytes());
        crate::canonical::check_total(&bytes)?;
        Ok(bytes)
    }

    /// Decodes and verifies the checksum and all field bounds.
    ///
    /// # Errors
    /// Rejects truncation, corruption, unknown versions/tags, and invalid decoded fields.
    pub fn decode(bytes: &[u8]) -> Result<Self, LinuxError> {
        if bytes.len() < 32 {
            return Err(manifest_error("manifest length is outside its bound"));
        }
        let split = bytes.len() - 32;
        let (body, checksum) = bytes.split_at(split);
        if peritus_codec::sha256(body).as_bytes() != checksum {
            return Err(manifest_error("manifest checksum mismatch"));
        }
        let mut reader = crate::canonical::Reader::new(body);
        if reader.fixed::<8>()? != MANIFEST_MAGIC {
            return Err(manifest_error("manifest magic or version is unsupported"));
        }
        let version = reader.u16()?;
        if !matches!(version, LEGACY_VERSION | VERSION)
            || (version == LEGACY_VERSION && bytes.len() > crate::canonical::MAX_PROTOCOL_BYTES)
        {
            return Err(manifest_error("manifest magic or version is unsupported"));
        }
        let plan_digest = Sha256Digest::new(reader.fixed()?);
        let backend_digest = Sha256Digest::new(reader.fixed()?);
        let support_digest = Sha256Digest::new(reader.fixed()?);
        let preparation_digest = Sha256Digest::new(reader.fixed()?);
        let target = if version == LEGACY_VERSION {
            let program = reader.string()?;
            let argument_count = reader.count()?;
            let mut arguments = Vec::with_capacity(argument_count);
            for _ in 0..argument_count {
                arguments.push(reader.string()?);
            }
            TargetCommand::new_legacy(program, arguments)?
        } else {
            let program = OsString::from_vec(reader.bytes_unbounded()?);
            let argument_count = reader.count_unbounded()?;
            let mut arguments = Vec::with_capacity(argument_count);
            for _ in 0..argument_count {
                arguments.push(OsString::from_vec(reader.bytes_unbounded()?));
            }
            TargetCommand::new(program, arguments)?
        };
        let working_directory = PathBuf::from(if version == LEGACY_VERSION {
            OsString::from(reader.string()?)
        } else {
            OsString::from_vec(reader.bytes_unbounded()?)
        });
        let cgroup_leaf = PathBuf::from(if version == LEGACY_VERSION {
            OsString::from(reader.string()?)
        } else {
            OsString::from_vec(reader.bytes_unbounded()?)
        });
        let pty = match reader.u8()? {
            0 => false,
            1 => true,
            _ => return Err(manifest_error("manifest PTY tag is invalid")),
        };
        let environment_count = if version == LEGACY_VERSION {
            reader.count()?
        } else {
            reader.count_unbounded()?
        };
        let mut environment = Vec::with_capacity(environment_count);
        for _ in 0..environment_count {
            let entry = if version == LEGACY_VERSION {
                EnvironmentEntry::new_legacy(reader.string()?, reader.string()?)?
            } else {
                EnvironmentEntry::new(
                    OsString::from_vec(reader.bytes_unbounded()?),
                    OsString::from_vec(reader.bytes_unbounded()?),
                )?
            };
            environment.push(entry);
        }
        let rule_count = reader.count()?;
        let mut landlock_rules = Vec::with_capacity(rule_count);
        for _ in 0..rule_count {
            landlock_rules.push(LandlockRule::new(
                PathBuf::from(if version == LEGACY_VERSION {
                    OsString::from(reader.string()?)
                } else {
                    OsString::from_vec(reader.bytes_unbounded()?)
                }),
                LandlockAccess::from_bits(reader.u16()?)?,
            )?);
        }
        let resources = ResourcePlan::decode(&mut reader)?;
        let network = match reader.u8()? {
            0 => NetworkIsolation::DenyAll,
            1 => NetworkIsolation::ManagedProxy,
            _ => return Err(manifest_error("manifest network tag is unknown")),
        };
        let handle_count = reader.count()?;
        let mut handles = Vec::with_capacity(handle_count);
        for _ in 0..handle_count {
            let descriptor = reader.u64()?;
            let label = if version == LEGACY_VERSION {
                reader.string()?
            } else {
                reader.string_unbounded()?
            };
            handles.push(InheritedHandle::new(descriptor, label)?);
        }
        let protected_count = reader.count()?;
        let mut protected_payloads = Vec::with_capacity(protected_count);
        for _ in 0..protected_count {
            let resource_id = ResourceId::new(reader.fixed::<16>()?)
                .map_err(|_| manifest_error("protected payload resource identity is invalid"))?;
            let payload_version = Sha256Digest::new(reader.fixed()?);
            let delivery = if version == LEGACY_VERSION {
                delivery::decode(&mut reader)?
            } else {
                delivery::decode_v2(&mut reader)?
            };
            let descriptor = reader.u64()?;
            let label = if version == LEGACY_VERSION {
                reader.string()?
            } else {
                reader.string_unbounded()?
            };
            let handle = InheritedHandle::new(descriptor, label)?;
            let payload_len = usize::try_from(reader.u32()?)
                .map_err(|_| manifest_error("protected payload length is invalid"))?;
            protected_payloads.push(ProtectedPayloadBinding::new(
                SecretGrant::new(SecretReference::new(resource_id, payload_version), delivery),
                handle,
                payload_len,
            )?);
        }
        reader.finish()?;
        let mut manifest = Self::new(
            plan_digest,
            backend_digest,
            support_digest,
            preparation_digest,
            target,
            working_directory,
            cgroup_leaf,
            pty,
            environment,
            landlock_rules,
            resources,
            network,
            handles,
        )?
        .with_protected_payloads(protected_payloads)?;
        manifest.encoding_version = version;
        Ok(manifest)
    }

    /// Returns the digest of complete encoded bytes.
    ///
    /// # Errors
    /// Returns an encoding error for an out-of-bound manifest.
    pub fn digest(&self) -> Result<Sha256Digest, LinuxError> {
        Ok(peritus_codec::sha256(&self.encode()?))
    }
}

fn valid_cgroup_leaf(leaf: &std::path::Path) -> bool {
    leaf.is_absolute()
        && leaf.parent().is_some()
        && leaf.file_name().and_then(|name| name.to_str()).is_some_and(|name| {
            name.starts_with("peritus-")
                && name.len() <= 128
                && name.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

fn manifest_error(detail: &'static str) -> LinuxError {
    LinuxError::new(
        LinuxErrorKind::Helper,
        LinuxOperation::Manifest,
        LinuxRecovery::CorrectRequest,
        detail,
    )
}

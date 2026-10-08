//! Trusted service-host managed-network grants for exact native gates.

use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fmt::Write as _,
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::Arc,
};

use peritus_network::{
    DnsMode, ManagedProxyPreparation, NetworkBounds, ProxyMode, RedirectMode, RoutingToken,
    RuntimeNetworkOptions, SystemResolver,
};
use peritus_sandbox::{
    CheckedSandboxPlan, DnsName, FileDecision, FileOperation, FileOperationSet, HostMatcher,
    NetworkContract, NetworkHost, NetworkRule, NetworkTarget, PathScope, PortRange, RuleEffect,
    SandboxPath, Transport,
};
use peritus_types::{Sha256Digest, WorkspaceId};

use crate::{ProductRunnerError, ProductRunnerErrorKind};

/// One exact TCP destination admitted by a trusted gate-network grant.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ManagedGateNetworkDestination {
    host: NetworkHost,
    port: u16,
}

impl ManagedGateNetworkDestination {
    /// Creates an exact canonical DNS destination.
    ///
    /// # Errors
    /// Rejects an invalid DNS name or port zero.
    pub fn dns(name: impl Into<String>, port: u16) -> Result<Self, ProductRunnerError> {
        let name = DnsName::new(name).map_err(|error| network_error(error.to_string()))?;
        Self::new(NetworkHost::Dns(name), port)
    }

    /// Creates an exact IP destination.
    ///
    /// # Errors
    /// Rejects port zero.
    pub fn ip(address: IpAddr, port: u16) -> Result<Self, ProductRunnerError> {
        Self::new(NetworkHost::Ip(address), port)
    }

    fn new(host: NetworkHost, port: u16) -> Result<Self, ProductRunnerError> {
        NetworkTarget::new(host.clone(), Transport::Tcp, port)
            .map_err(|error| network_error(error.to_string()))?;
        Ok(Self { host, port })
    }

    /// Returns the canonical destination host.
    #[must_use]
    pub const fn host(&self) -> &NetworkHost {
        &self.host
    }

    /// Returns the exact TCP port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GrantSource {
    Configured,
    CargoRegistry,
    GoModules,
}

impl GrantSource {
    const fn tag(self) -> u8 {
        match self {
            Self::Configured => 0,
            Self::CargoRegistry => 1,
            Self::GoModules => 2,
        }
    }

    fn from_tag(tag: u8) -> Result<Self, ProductRunnerError> {
        match tag {
            0 => Ok(Self::Configured),
            1 => Ok(Self::CargoRegistry),
            2 => Ok(Self::GoModules),
            _ => Err(network_error("managed gate network grant has an unknown source")),
        }
    }

    const fn standard_program(self) -> Option<&'static str> {
        match self {
            Self::Configured => None,
            Self::CargoRegistry => Some("cargo"),
            Self::GoModules => Some("go"),
        }
    }
}

/// Canonical nonsensitive semantic grant supplied by the trusted service host.
///
/// Routing tokens, resolver state, sockets, credentials, and native handles are created only
/// after process authority and are never part of this value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedGateNetworkGrant {
    workspace_id: WorkspaceId,
    base_command: Sha256Digest,
    destinations: Vec<ManagedGateNetworkDestination>,
    options: RuntimeNetworkOptions,
    windows_controller: Sha256Digest,
    source: GrantSource,
    canonical: Vec<u8>,
    digest: Sha256Digest,
}

impl ManagedGateNetworkGrant {
    /// Creates one strict configured host grant.
    ///
    /// # Errors
    /// Rejects an empty destination set, duplicate/invalid destinations, or zero controller
    /// identity. Runtime work allowances remain absent; physical proxy windows are separate.
    pub fn configured(
        workspace_id: WorkspaceId,
        base_command: Sha256Digest,
        destinations: Vec<ManagedGateNetworkDestination>,
        windows_controller: Option<Sha256Digest>,
    ) -> Result<Self, ProductRunnerError> {
        Self::new(
            workspace_id,
            base_command,
            destinations,
            windows_controller.unwrap_or_else(default_windows_controller),
            GrantSource::Configured,
        )
    }

    pub(crate) fn standard(
        workspace_id: WorkspaceId,
        base_command: Sha256Digest,
        program: &str,
    ) -> Result<Option<Self>, ProductRunnerError> {
        let (source, names): (GrantSource, &[(&str, u16)]) = match program {
            "cargo" => (
                GrantSource::CargoRegistry,
                &[
                    ("crates.io", 443),
                    ("index.crates.io", 443),
                    ("static.crates.io", 443),
                    ("github.com", 443),
                    ("objects.githubusercontent.com", 443),
                ],
            ),
            "go" => (
                GrantSource::GoModules,
                &[
                    ("proxy.golang.org", 443),
                    ("sum.golang.org", 443),
                    ("storage.googleapis.com", 443),
                ],
            ),
            _ => return Ok(None),
        };
        let destinations = names
            .iter()
            .map(|(name, port)| ManagedGateNetworkDestination::dns(*name, *port))
            .collect::<Result<Vec<_>, _>>()?;
        Self::new(
            workspace_id,
            base_command,
            destinations,
            default_windows_controller(),
            source,
        )
        .map(Some)
    }

    fn new(
        workspace_id: WorkspaceId,
        base_command: Sha256Digest,
        mut destinations: Vec<ManagedGateNetworkDestination>,
        windows_controller: Sha256Digest,
        source: GrantSource,
    ) -> Result<Self, ProductRunnerError> {
        destinations.sort();
        if destinations.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(network_error(
                "managed gate network grant repeats an exact destination",
            ));
        }
        destinations.dedup();
        if destinations.is_empty() || windows_controller == Sha256Digest::new([0; 32]) {
            return Err(network_error(
                "managed gate network grant is empty or has a zero controller identity",
            ));
        }
        let bounds = NetworkBounds::without_limits();
        let options = RuntimeNetworkOptions::new(
            DnsMode::ProxySystem,
            RedirectMode::Deny,
            ProxyMode::HttpConnect,
            bounds,
            Vec::new(),
        );
        let canonical = canonical_grant(
            workspace_id,
            base_command,
            &destinations,
            windows_controller,
            source,
        )?;
        let digest = peritus_codec::sha256(&canonical);
        Ok(Self {
            workspace_id,
            base_command,
            destinations,
            options,
            windows_controller,
            source,
            canonical,
            digest,
        })
    }

    /// Returns the workspace whose gates may consume this grant.
    #[must_use]
    pub const fn workspace_id(&self) -> WorkspaceId {
        self.workspace_id
    }

    /// Returns the exact denied-network command identity selected by the host.
    #[must_use]
    pub const fn base_command_digest(&self) -> Sha256Digest {
        self.base_command
    }

    /// Returns exact canonical TCP destinations.
    #[must_use]
    pub fn destinations(&self) -> &[ManagedGateNetworkDestination] {
        &self.destinations
    }

    /// Returns the reviewed Windows WFP controller identity.
    #[must_use]
    pub const fn windows_controller_digest(&self) -> Sha256Digest {
        self.windows_controller
    }

    /// Returns the complete versioned nonsensitive semantic grant bytes.
    #[must_use]
    pub fn canonical_bytes(&self) -> &[u8] {
        &self.canonical
    }

    /// Returns the complete canonical semantic grant digest.
    #[must_use]
    pub const fn digest(&self) -> Sha256Digest {
        self.digest
    }

    /// Reports whether this grant is one of the service's closed built-in toolchain grants.
    #[must_use]
    pub const fn is_builtin(&self) -> bool {
        !matches!(self.source, GrantSource::Configured)
    }

    pub(super) fn network_contract(&self) -> Result<NetworkContract, ProductRunnerError> {
        let rules = self
            .destinations
            .iter()
            .map(|destination| {
                let matcher = match &destination.host {
                    NetworkHost::Dns(name) => HostMatcher::DnsExact(name.clone()),
                    NetworkHost::Ip(address) => HostMatcher::ip_prefix(
                        *address,
                        if address.is_ipv4() { 32 } else { 128 },
                    )
                    .map_err(|error| network_error(error.to_string()))?,
                };
                let ports = PortRange::new(destination.port, destination.port)
                    .map_err(|error| network_error(error.to_string()))?;
                Ok(NetworkRule::new(RuleEffect::Allow, matcher, Transport::Tcp, ports))
            })
            .collect::<Result<Vec<_>, ProductRunnerError>>()?;
        NetworkContract::new(rules).map_err(|error| network_error(error.to_string()))
    }

    pub(super) fn network_requirements(&self) -> Result<Vec<NetworkTarget>, ProductRunnerError> {
        self.destinations
            .iter()
            .map(|destination| {
                NetworkTarget::new(destination.host.clone(), Transport::Tcp, destination.port)
                    .map_err(|error| network_error(error.to_string()))
            })
            .collect()
    }

    /// Creates a fresh protected proxy preparation for this already revalidated grant.
    ///
    /// The returned owner keeps its routing token and resolver opaque. Nothing ephemeral is
    /// encoded by [`Self::canonical_bytes`] or exposed through the retained request.
    ///
    /// # Errors
    /// Returns a typed host failure if fresh route entropy is unavailable.
    pub fn fresh_proxy_preparation(
        &self,
    ) -> Result<ManagedProxyPreparation, ProductRunnerError> {
        let mut token = [0_u8; 32];
        getrandom::fill(&mut token)
            .map_err(|error| network_error(format!("generate managed proxy route: {error}")))?;
        Ok(ManagedProxyPreparation::new(
            self.options.clone(),
            RoutingToken::new(token),
            Arc::new(SystemResolver),
            None,
        ))
    }

    /// Opens the exact run-owned cache selected by this grant beneath canonical command state.
    ///
    /// The grant digest is the only child name. Existing aliases and paths outside the retained
    /// run's command state are rejected before the path is supplied to native preparation.
    ///
    /// # Errors
    /// Returns a typed storage or identity failure when the root/cache cannot be established
    /// exactly.
    pub fn open_run_owned_cache(
        &self,
        command_state_root: &Path,
    ) -> Result<PathBuf, ProductRunnerError> {
        let (canonical_root, path) = self.run_owned_cache_path(command_state_root)?;
        fs::create_dir_all(&path)
            .map_err(|error| cache_error(format!("create run-owned managed gate cache: {error}")))?;
        let canonical = path
            .canonicalize()
            .map_err(|error| cache_error(format!("resolve run-owned managed gate cache: {error}")))?;
        if canonical != path || !canonical.starts_with(&canonical_root) || !canonical.is_dir() {
            return Err(cache_error(
                "run-owned managed gate cache contains an alias or escapes command state",
            ));
        }
        Ok(canonical)
    }

    /// Restores the exact run-owned cache only when the retained checked plan binds it.
    ///
    /// This is the parent-reconstruction boundary. `command_state_root` comes from the exact
    /// retained backend-factory payload, not from process environment or path ancestry guesses.
    /// The grant's complete network contract/requirements and the cache's complete writable
    /// filesystem rule/requirement must match before the cache is opened.
    ///
    /// # Errors
    /// Rejects a different grant projection, missing writable cache authority, aliases, or
    /// storage failure.
    pub fn restore_run_owned_cache(
        &self,
        command_state_root: &Path,
        checked: &CheckedSandboxPlan,
    ) -> Result<PathBuf, ProductRunnerError> {
        let (_, path) = self.run_owned_cache_path(command_state_root)?;
        let logical = native_cache_path(path.as_os_str())?;
        let operations = FileOperationSet::from_operations([
            FileOperation::Discover,
            FileOperation::Metadata,
            FileOperation::Read,
            FileOperation::Execute,
            FileOperation::Create,
            FileOperation::Write,
            FileOperation::Remove,
        ]);
        let exact_rule = checked.contract().filesystem().rules().iter().any(|rule| {
            rule.effect() == RuleEffect::Allow
                && rule.path() == &logical
                && rule.scope() == PathScope::Descendants
                && rule.operations() == operations
        });
        let exact_requirement = checked
            .requirements()
            .files()
            .iter()
            .any(|requirement| {
                requirement.path() == &logical
                    && requirement.operation() == FileOperation::Write
            });
        let every_operation_allowed = [
            FileOperation::Discover,
            FileOperation::Metadata,
            FileOperation::Read,
            FileOperation::Execute,
            FileOperation::Create,
            FileOperation::Write,
            FileOperation::Remove,
        ]
        .into_iter()
        .all(|operation| {
            checked.contract().filesystem().decide(&logical, operation)
                == FileDecision::Allowed
        });
        let expected_network = self.network_contract()?;
        let expected_requirements = self.network_requirements()?;
        if !exact_rule
            || !exact_requirement
            || !every_operation_allowed
            || checked.contract().network() != &expected_network
            || checked.requirements().network() != expected_requirements.as_slice()
        {
            return Err(cache_error(
                "retained checked plan does not bind the exact managed grant and writable cache",
            ));
        }
        self.open_run_owned_cache(command_state_root)
    }

    fn run_owned_cache_path(
        &self,
        command_state_root: &Path,
    ) -> Result<(PathBuf, PathBuf), ProductRunnerError> {
        let canonical_root = command_state_root
            .canonicalize()
            .map_err(|error| cache_error(format!("resolve command state root: {error}")))?;
        if canonical_root != command_state_root || !canonical_root.is_dir() {
            return Err(cache_error(
                "command state root is not one canonical run-owned directory",
            ));
        }
        let path = canonical_root
            .join("managed-gate-cache-v1")
            .join(self.cache_key());
        Ok((canonical_root, path))
    }

    fn cache_key(&self) -> String {
        let mut output = String::with_capacity(Sha256Digest::LENGTH * 2);
        for byte in self.digest.as_bytes() {
            write!(&mut output, "{byte:02x}")
                .expect("writing hexadecimal into String cannot fail");
        }
        output
    }
}

/// Immutable trusted grant lookup assembled by the service host.
#[derive(Clone, Debug, Default)]
pub struct ManagedGateNetworkCatalog {
    grants: Arc<BTreeMap<(WorkspaceId, Sha256Digest), ManagedGateNetworkGrant>>,
    by_digest: Arc<BTreeMap<Sha256Digest, ManagedGateNetworkGrant>>,
}

impl ManagedGateNetworkCatalog {
    /// Validates an exact configured catalog.
    ///
    /// # Errors
    /// Rejects duplicate command selectors or one digest resolving to unequal semantic grants.
    pub fn new(
        grants: impl IntoIterator<Item = ManagedGateNetworkGrant>,
    ) -> Result<Self, ProductRunnerError> {
        let mut selected = BTreeMap::new();
        let mut by_digest = BTreeMap::new();
        for grant in grants {
            let key = (grant.workspace_id, grant.base_command);
            if selected.insert(key, grant.clone()).is_some() {
                return Err(network_error(
                    "managed gate network selector is configured more than once",
                ));
            }
            if let Some(previous) = by_digest.insert(grant.digest, grant.clone())
                && previous != grant
            {
                return Err(network_error(
                    "managed gate network digest resolves to unequal grants",
                ));
            }
        }
        Ok(Self { grants: Arc::new(selected), by_digest: Arc::new(by_digest) })
    }

    /// Resolves one exact configured command grant.
    #[must_use]
    pub fn resolve(
        &self,
        workspace_id: WorkspaceId,
        base_command: Sha256Digest,
    ) -> Option<ManagedGateNetworkGrant> {
        self.grants.get(&(workspace_id, base_command)).cloned()
    }

    /// Selects trusted managed egress for one exact host-planned command.
    ///
    /// Configured selectors take precedence. When the live Network capability is present, the
    /// service's closed Cargo and Go grants keep ordinary registry/module fetches usable without
    /// requiring a per-command catalog entry. No executable name supplied by candidate content
    /// can widen either built-in destination set.
    ///
    /// # Errors
    /// Returns a typed configuration error if a built-in semantic grant cannot be constructed.
    pub fn resolve_command(
        &self,
        workspace_id: WorkspaceId,
        base_command: Sha256Digest,
        program: &str,
        network_authorized: bool,
    ) -> Result<Option<ManagedGateNetworkGrant>, ProductRunnerError> {
        if !network_authorized {
            return Ok(None);
        }
        if let Some(grant) = self.resolve(workspace_id, base_command) {
            return Ok(Some(grant));
        }
        ManagedGateNetworkGrant::standard(workspace_id, base_command, program)
    }

    /// Resolves a retained configured semantic grant by complete digest.
    #[must_use]
    pub fn resolve_digest(&self, digest: Sha256Digest) -> Option<ManagedGateNetworkGrant> {
        self.by_digest.get(&digest).cloned()
    }

    /// Revalidates a retained nonsensitive semantic grant against current trusted host policy.
    ///
    /// Configured grants must still exist byte-for-byte in this catalog. Built-in grants are
    /// reconstructed from their closed source tag and compared with the complete retained bytes.
    /// Tokens, resolver state, credentials, listeners, and native handles are never decoded here.
    ///
    /// # Errors
    /// Rejects malformed bytes, digest mismatch, removed configured authority, or any drift from
    /// the current built-in grant definition.
    pub fn resolve_retained(
        &self,
        canonical: &[u8],
        digest: Sha256Digest,
    ) -> Result<ManagedGateNetworkGrant, ProductRunnerError> {
        if peritus_codec::sha256(canonical) != digest {
            return Err(network_error("retained managed gate grant digest differs"));
        }
        let decoded = decode_grant(canonical)?;
        let trusted = if let Some(program) = decoded.source.standard_program() {
            ManagedGateNetworkGrant::standard(
                decoded.workspace_id,
                decoded.base_command,
                program,
            )?
            .ok_or_else(|| network_error("retained built-in grant cannot be reconstructed"))?
        } else {
            self.resolve_digest(digest).ok_or_else(|| {
                network_error("retained configured managed gate grant is no longer authorized")
            })?
        };
        if trusted.canonical_bytes() != canonical || trusted.digest() != digest {
            return Err(network_error(
                "retained managed gate grant differs from current trusted host policy",
            ));
        }
        Ok(trusted)
    }
}

struct DecodedGrant {
    workspace_id: WorkspaceId,
    base_command: Sha256Digest,
    source: GrantSource,
}

fn decode_grant(bytes: &[u8]) -> Result<DecodedGrant, ProductRunnerError> {
    const PREFIX: &[u8] = b"PERITUS_MANAGED_GATE_NETWORK_GRANT\0\x01";
    let mut decoder = GrantDecoder::new(bytes);
    decoder.expect(PREFIX)?;
    let workspace_id = WorkspaceId::new(decoder.array()?)
        .map_err(|_| network_error("managed gate network grant has a zero workspace identity"))?;
    let base_command = Sha256Digest::new(decoder.array()?);
    let source = GrantSource::from_tag(decoder.byte()?)?;
    let count = decoder.u32()?;
    for _ in 0..count {
        match decoder.byte()? {
            0 => {
                let name = decoder.bytes()?;
                let name = std::str::from_utf8(name)
                    .map_err(|_| network_error("managed gate DNS destination is not UTF-8"))?;
                let _ = ManagedGateNetworkDestination::dns(name, decoder.u16()?)?;
            }
            4 => {
                let address = std::net::Ipv4Addr::from(decoder.array::<4>()?);
                let _ = ManagedGateNetworkDestination::ip(address.into(), decoder.u16()?)?;
            }
            6 => {
                let address = std::net::Ipv6Addr::from(decoder.array::<16>()?);
                let _ = ManagedGateNetworkDestination::ip(address.into(), decoder.u16()?)?;
            }
            _ => return Err(network_error("managed gate network host tag is invalid")),
        }
    }
    if decoder.array::<5>()? != [0; 5] {
        return Err(network_error(
            "managed gate network runtime policy is not the supported version-one policy",
        ));
    }
    let _: [u8; 32] = decoder.array()?;
    decoder.finish()?;
    Ok(DecodedGrant { workspace_id, base_command, source })
}

struct GrantDecoder<'a> {
    remaining: &'a [u8],
}

impl<'a> GrantDecoder<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { remaining: bytes }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ProductRunnerError> {
        if self.remaining.len() < length {
            return Err(network_error("managed gate network grant is truncated"));
        }
        let (value, remaining) = self.remaining.split_at(length);
        self.remaining = remaining;
        Ok(value)
    }

    fn expect(&mut self, expected: &[u8]) -> Result<(), ProductRunnerError> {
        if self.take(expected.len())? != expected {
            return Err(network_error("managed gate network grant version is unsupported"));
        }
        Ok(())
    }

    fn byte(&mut self) -> Result<u8, ProductRunnerError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, ProductRunnerError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, ProductRunnerError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ProductRunnerError> {
        self.take(N)?
            .try_into()
            .map_err(|_| network_error("managed gate network field length is invalid"))
    }

    fn bytes(&mut self) -> Result<&'a [u8], ProductRunnerError> {
        let length = usize::try_from(self.u32()?)
            .map_err(|_| network_error("managed gate network field length is unrepresentable"))?;
        self.take(length)
    }

    fn finish(self) -> Result<(), ProductRunnerError> {
        if self.remaining.is_empty() {
            Ok(())
        } else {
            Err(network_error("managed gate network grant has trailing bytes"))
        }
    }
}

fn canonical_grant(
    workspace_id: WorkspaceId,
    base_command: Sha256Digest,
    destinations: &[ManagedGateNetworkDestination],
    windows_controller: Sha256Digest,
    source: GrantSource,
) -> Result<Vec<u8>, ProductRunnerError> {
    let mut bytes = Vec::from(b"PERITUS_MANAGED_GATE_NETWORK_GRANT\0\x01".as_slice());
    bytes.extend_from_slice(workspace_id.as_bytes());
    bytes.extend_from_slice(base_command.as_bytes());
    bytes.push(source.tag());
    let count = u32::try_from(destinations.len())
        .map_err(|_| network_error("managed gate destination count exceeds u32"))?;
    bytes.extend_from_slice(&count.to_be_bytes());
    for destination in destinations {
        match &destination.host {
            NetworkHost::Dns(name) => {
                bytes.push(0);
                put_bytes(&mut bytes, name.as_str().as_bytes())?;
            }
            NetworkHost::Ip(IpAddr::V4(address)) => {
                bytes.push(4);
                bytes.extend_from_slice(&address.octets());
            }
            NetworkHost::Ip(IpAddr::V6(address)) => {
                bytes.push(6);
                bytes.extend_from_slice(&address.octets());
            }
        }
        bytes.extend_from_slice(&destination.port.to_be_bytes());
    }
    // ProxySystem, redirect deny, HTTP CONNECT, no selected lifetime/work bounds, no credentials,
    // and one stable run-owned cache are closed semantics of version one.
    bytes.extend_from_slice(&[0, 0, 0, 0, 0]);
    bytes.extend_from_slice(windows_controller.as_bytes());
    Ok(bytes)
}

fn put_bytes(bytes: &mut Vec<u8>, value: &[u8]) -> Result<(), ProductRunnerError> {
    let len = u32::try_from(value.len())
        .map_err(|_| network_error("managed gate network field exceeds u32"))?;
    bytes.extend_from_slice(&len.to_be_bytes());
    bytes.extend_from_slice(value);
    Ok(())
}

fn default_windows_controller() -> Sha256Digest {
    peritus_codec::sha256(b"peritus-windows-managed-wfp-controller-v1\0")
}

fn native_cache_path(value: &OsStr) -> Result<SandboxPath, ProductRunnerError> {
    if let Some(text) = value.to_str() {
        let normalized = text.replace('\\', "/");
        if let Ok(path) = SandboxPath::new(normalized) {
            return Ok(path);
        }
    }
    let mut bytes = Vec::new();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        bytes.push(1);
        bytes.extend_from_slice(value.as_bytes());
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt as _;
        bytes.push(2);
        for unit in value.encode_wide() {
            bytes.extend_from_slice(&unit.to_be_bytes());
        }
    }
    let digest = peritus_codec::sha256(&bytes);
    let mut encoded = String::with_capacity(digest.as_bytes().len() * 2);
    for byte in digest.as_bytes() {
        write!(&mut encoded, "{byte:02x}")
            .expect("writing hexadecimal into String cannot fail");
    }
    SandboxPath::new(format!("/__peritus_native/managed-cache/{encoded}"))
        .map_err(|error| cache_error(format!("construct retained managed cache path: {error}")))
}

fn network_error(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::InvalidPrecondition,
        "configure managed gate network",
        detail.into(),
    )
}

fn cache_error(detail: impl Into<String>) -> ProductRunnerError {
    ProductRunnerError::new(
        ProductRunnerErrorKind::Apply,
        "open run-owned managed gate cache",
        detail.into(),
    )
}

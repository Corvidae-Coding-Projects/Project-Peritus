//! Deny-by-default network capability contracts.

use crate::SandboxError;
use std::net::IpAddr;

const MAX_RULES: usize = 256;

/// A normalized ASCII DNS name.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DnsName(String);

impl DnsName {
    /// Validates and lowercases a DNS name.
    ///
    /// # Errors
    /// Rejects empty, oversized, malformed, or non-ASCII names.
    pub fn new(value: impl Into<String>) -> Result<Self, SandboxError> {
        let mut value = value.into();
        if value.ends_with('.') {
            value.pop();
        }
        value.make_ascii_lowercase();
        let valid = !value.is_empty()
            && value.len() <= 253
            && value.is_ascii()
            && value.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && !label.starts_with('-')
                    && !label.ends_with('-')
                    && label.bytes().all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            });
        if !valid {
            return Err(crate::error::invalid("invalid DNS name"));
        }
        Ok(Self(value))
    }

    /// Returns canonical lowercase text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Transport protocol governed by a network rule.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Transport {
    /// Transmission Control Protocol.
    Tcp,
    /// User Datagram Protocol.
    Udp,
}

impl Transport {
    pub(crate) const fn ordinal(self) -> u8 {
        match self {
            Self::Tcp => 0,
            Self::Udp => 1,
        }
    }
}

/// Inclusive port range.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PortRange {
    start: u16,
    end: u16,
}

impl PortRange {
    /// Creates an inclusive range.
    ///
    /// # Errors
    /// Rejects zero ports and reversed ranges.
    pub const fn new(start: u16, end: u16) -> Result<Self, SandboxError> {
        if start == 0 || start > end {
            return Err(crate::error::invalid("invalid network port range"));
        }
        Ok(Self { start, end })
    }
    /// Returns the first port.
    #[must_use]
    pub const fn start(self) -> u16 {
        self.start
    }
    /// Returns the final port.
    #[must_use]
    pub const fn end(self) -> u16 {
        self.end
    }
    const fn contains(self, port: u16) -> bool {
        port >= self.start && port <= self.end
    }
}

/// A host supplied by an attempted connection.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NetworkHost {
    /// A validated DNS name.
    Dns(DnsName),
    /// A parsed IP address.
    Ip(IpAddr),
}

/// A host selector in a network rule.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum HostMatcher {
    /// Match exactly one DNS name.
    DnsExact(DnsName),
    /// Match the suffix itself and its subdomains.
    DnsSuffix(DnsName),
    /// Match an IP prefix with a validated prefix length.
    IpPrefix {
        /// Canonical network address with host bits cleared.
        address: IpAddr,
        /// Prefix length in bits.
        prefix_length: u8,
    },
}

impl HostMatcher {
    /// Creates an IP prefix matcher.
    ///
    /// # Errors
    /// Rejects lengths greater than 32 for IPv4 or 128 for IPv6.
    pub fn ip_prefix(address: IpAddr, prefix_length: u8) -> Result<Self, SandboxError> {
        let maximum = if address.is_ipv4() { 32 } else { 128 };
        if prefix_length > maximum {
            return Err(crate::error::invalid("invalid IP prefix length"));
        }
        let address = normalized_prefix(address, prefix_length);
        Ok(Self::IpPrefix { address, prefix_length })
    }

    fn matches(&self, host: &NetworkHost) -> bool {
        match (self, host) {
            (Self::DnsExact(expected), NetworkHost::Dns(actual)) => expected == actual,
            (Self::DnsSuffix(expected), NetworkHost::Dns(actual)) => {
                actual == expected
                    || actual
                        .as_str()
                        .strip_suffix(expected.as_str())
                        .is_some_and(|prefix| prefix.ends_with('.'))
            }
            (Self::IpPrefix { address, prefix_length }, NetworkHost::Ip(actual)) => {
                ip_prefix_matches(*address, *actual, *prefix_length)
            }
            _ => false,
        }
    }
}

fn normalized_prefix(address: IpAddr, length: u8) -> IpAddr {
    match address {
        IpAddr::V4(value) => {
            let mask = if length == 0 { 0 } else { u32::MAX << (32 - length) };
            IpAddr::V4(std::net::Ipv4Addr::from(u32::from(value) & mask))
        }
        IpAddr::V6(value) => {
            let mask = if length == 0 { 0 } else { u128::MAX << (128 - length) };
            IpAddr::V6(std::net::Ipv6Addr::from(u128::from(value) & mask))
        }
    }
}

fn ip_prefix_matches(expected: IpAddr, actual: IpAddr, length: u8) -> bool {
    match (expected, actual) {
        (IpAddr::V4(left), IpAddr::V4(right)) => {
            let mask = if length == 0 { 0 } else { u32::MAX << (32 - length) };
            u32::from(left) & mask == u32::from(right) & mask
        }
        (IpAddr::V6(left), IpAddr::V6(right)) => {
            let mask = if length == 0 { 0 } else { u128::MAX << (128 - length) };
            u128::from(left) & mask == u128::from(right) & mask
        }
        _ => false,
    }
}

/// A concrete connection target.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NetworkTarget {
    host: NetworkHost,
    transport: Transport,
    port: u16,
}

impl NetworkTarget {
    /// Creates a target.
    ///
    /// # Errors
    /// Rejects port zero.
    pub fn new(host: NetworkHost, transport: Transport, port: u16) -> Result<Self, SandboxError> {
        if port == 0 {
            return Err(crate::error::invalid("network port must be nonzero"));
        }
        Ok(Self { host, transport, port })
    }
    /// Returns the host.
    #[must_use]
    pub const fn host(&self) -> &NetworkHost {
        &self.host
    }
    /// Returns the transport.
    #[must_use]
    pub const fn transport(&self) -> Transport {
        self.transport
    }
    /// Returns the port.
    #[must_use]
    pub const fn port(&self) -> u16 {
        self.port
    }
}

/// One allow or deny network rule.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NetworkRule {
    effect: crate::RuleEffect,
    host: HostMatcher,
    transport: Transport,
    ports: PortRange,
}

impl NetworkRule {
    /// Creates a network rule.
    #[must_use]
    pub const fn new(
        effect: crate::RuleEffect,
        host: HostMatcher,
        transport: Transport,
        ports: PortRange,
    ) -> Self {
        Self { effect, host, transport, ports }
    }
    /// Returns the effect.
    #[must_use]
    pub const fn effect(&self) -> crate::RuleEffect {
        self.effect
    }
    /// Returns the host matcher.
    #[must_use]
    pub const fn host(&self) -> &HostMatcher {
        &self.host
    }
    /// Returns the transport.
    #[must_use]
    pub const fn transport(&self) -> Transport {
        self.transport
    }
    /// Returns the ports.
    #[must_use]
    pub const fn ports(&self) -> PortRange {
        self.ports
    }
}

/// Network decision with an explicit default-deny distinction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkDecision {
    /// An allow rule matched and no deny rule matched.
    Allowed,
    /// A deny rule matched.
    DeniedByRule,
    /// No rule allowed the target.
    DeniedByDefault,
}

/// Canonical network contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NetworkContract {
    rules: Vec<NetworkRule>,
}

impl NetworkContract {
    /// Validates, sorts, and deduplicates network rules.
    ///
    /// # Errors
    /// Returns a limit error for more than 256 rules.
    pub fn new(mut rules: Vec<NetworkRule>) -> Result<Self, SandboxError> {
        if rules.len() > MAX_RULES {
            return Err(crate::error::bound("too many network rules"));
        }
        rules.sort();
        rules.dedup();
        Ok(Self { rules })
    }
    /// Returns an empty, deny-all contract.
    #[must_use]
    pub const fn deny_all() -> Self {
        Self { rules: Vec::new() }
    }
    /// Returns canonical rules.
    #[must_use]
    pub fn rules(&self) -> &[NetworkRule] {
        &self.rules
    }
    /// Evaluates a target using deny precedence.
    #[must_use]
    pub fn decide(&self, target: &NetworkTarget) -> NetworkDecision {
        let mut allowed = false;
        for rule in &self.rules {
            if rule.transport == target.transport
                && rule.ports.contains(target.port)
                && rule.host.matches(&target.host)
            {
                match rule.effect {
                    crate::RuleEffect::Deny => return NetworkDecision::DeniedByRule,
                    crate::RuleEffect::Allow => allowed = true,
                }
            }
        }
        if allowed { NetworkDecision::Allowed } else { NetworkDecision::DeniedByDefault }
    }
}

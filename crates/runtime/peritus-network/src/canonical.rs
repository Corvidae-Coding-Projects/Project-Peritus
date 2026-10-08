//! Versioned canonical network-plan encoding.

use peritus_sandbox::{HostMatcher, NetworkRule, RuleEffect, SecretReference, Transport};
use peritus_types::{ProcessId, Sha256Digest};

use crate::{DnsMode, NetworkError, ProxyMode, RedirectMode, RuntimeNetworkOptions};

pub fn plan_bytes(
    owner: ProcessId,
    sandbox_digest: Sha256Digest,
    rules: &[NetworkRule],
    options: &RuntimeNetworkOptions,
) -> Result<Vec<u8>, NetworkError> {
    let mut out = Vec::new();
    let legacy = options.bounds().uses_legacy_encoding();
    out.extend_from_slice(if legacy {
        b"PERITUS_NETWORK_PLAN\0\x01"
    } else {
        b"PERITUS_NETWORK_PLAN\0\x02"
    });
    out.extend_from_slice(owner.as_bytes());
    out.extend_from_slice(sandbox_digest.as_bytes());
    put_u32(&mut out, rules.len())?;
    for rule in rules {
        out.push(match rule.effect() {
            RuleEffect::Allow => 1,
            RuleEffect::Deny => 0,
        });
        out.push(match rule.transport() {
            Transport::Tcp => 0,
            Transport::Udp => 1,
        });
        out.extend_from_slice(&rule.ports().start().to_be_bytes());
        out.extend_from_slice(&rule.ports().end().to_be_bytes());
        encode_matcher(&mut out, rule.host())?;
    }
    out.push(match options.dns() {
        DnsMode::ProxySystem => 0,
    });
    match options.redirects() {
        RedirectMode::Deny => out.extend_from_slice(&[0, 0]),
        RedirectMode::Follow { maximum } => out.extend_from_slice(&[1, maximum]),
    }
    out.push(match options.proxy() {
        ProxyMode::HttpConnect => 0,
    });
    let bounds = options.bounds();
    if legacy {
        encode_legacy_bounds(&mut out, bounds)?;
    } else {
        for value in [
            bounds.maximum_connections(),
            bounds.maximum_workers(),
            bounds.connection_bytes(),
            bounds.total_bytes(),
            bounds.connection_millis(),
            bounds.total_millis(),
            bounds.observations(),
            bounds.header_bytes(),
        ] {
            encode_optional_u64(&mut out, value);
        }
    }
    put_u32(&mut out, options.credentials().len())?;
    for reference in options.credentials() {
        encode_reference(&mut out, *reference);
    }
    Ok(out)
}

fn encode_legacy_bounds(
    out: &mut Vec<u8>,
    bounds: crate::NetworkBounds,
) -> Result<(), NetworkError> {
    let connections = u16::try_from(selected(bounds.maximum_connections())?)
        .map_err(|_| crate::error::invalid("legacy connection bound is not representable"))?;
    let workers = u16::try_from(selected(bounds.maximum_workers())?)
        .map_err(|_| crate::error::invalid("legacy worker bound is not representable"))?;
    let observations = u32::try_from(selected(bounds.observations())?)
        .map_err(|_| crate::error::invalid("legacy observation bound is not representable"))?;
    let header_bytes = u32::try_from(selected(bounds.header_bytes())?)
        .map_err(|_| crate::error::invalid("legacy header bound is not representable"))?;
    out.extend_from_slice(&connections.to_be_bytes());
    out.extend_from_slice(&workers.to_be_bytes());
    out.extend_from_slice(&selected(bounds.connection_bytes())?.to_be_bytes());
    out.extend_from_slice(&selected(bounds.total_bytes())?.to_be_bytes());
    out.extend_from_slice(&selected(bounds.connection_millis())?.to_be_bytes());
    out.extend_from_slice(&selected(bounds.total_millis())?.to_be_bytes());
    out.extend_from_slice(&observations.to_be_bytes());
    out.extend_from_slice(&header_bytes.to_be_bytes());
    Ok(())
}

fn selected(value: Option<core::num::NonZeroU64>) -> Result<u64, NetworkError> {
    value
        .map(core::num::NonZeroU64::get)
        .ok_or_else(|| crate::error::invalid("legacy network bound is absent"))
}

fn encode_optional_u64(out: &mut Vec<u8>, value: Option<core::num::NonZeroU64>) {
    match value {
        Some(value) => {
            out.push(1);
            out.extend_from_slice(&value.get().to_be_bytes());
        }
        None => out.push(0),
    }
}

fn encode_matcher(out: &mut Vec<u8>, matcher: &HostMatcher) -> Result<(), NetworkError> {
    match matcher {
        HostMatcher::DnsExact(name) => {
            out.push(0);
            put_bytes(out, name.as_str().as_bytes())?;
        }
        HostMatcher::DnsSuffix(name) => {
            out.push(1);
            put_bytes(out, name.as_str().as_bytes())?;
        }
        HostMatcher::IpPrefix { address, prefix_length } => {
            out.push(2);
            out.push(*prefix_length);
            match address {
                std::net::IpAddr::V4(value) => {
                    out.push(4);
                    out.extend_from_slice(&value.octets());
                }
                std::net::IpAddr::V6(value) => {
                    out.push(6);
                    out.extend_from_slice(&value.octets());
                }
            }
        }
    }
    Ok(())
}

fn encode_reference(out: &mut Vec<u8>, reference: SecretReference) {
    out.extend_from_slice(reference.resource_id().as_bytes());
    out.extend_from_slice(reference.version().as_bytes());
}

fn put_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), NetworkError> {
    put_u32(out, bytes.len())?;
    out.extend_from_slice(bytes);
    Ok(())
}

fn put_u32(out: &mut Vec<u8>, value: usize) -> Result<(), NetworkError> {
    let value = u32::try_from(value)
        .map_err(|_| crate::error::invalid("canonical network collection exceeds u32"))?;
    out.extend_from_slice(&value.to_be_bytes());
    Ok(())
}

//! Versioned canonical network-plan encoding.

use peritus_sandbox::{HostMatcher, NetworkRule, RuleEffect, SecretReference, Transport};
use peritus_types::{ProcessId, Sha256Digest};

use crate::{DnsMode, NetworkError, ProxyMode, RedirectMode, RuntimeNetworkOptions};

const MAX_CANONICAL_BYTES: usize = 2 * 1_024 * 1_024;

pub fn plan_bytes(
    owner: ProcessId,
    sandbox_digest: Sha256Digest,
    rules: &[NetworkRule],
    options: &RuntimeNetworkOptions,
) -> Result<Vec<u8>, NetworkError> {
    let mut out = Vec::new();
    out.extend_from_slice(b"PERITUS_NETWORK_PLAN\0\x01");
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
    out.extend_from_slice(&bounds.maximum_connections().to_be_bytes());
    out.extend_from_slice(&bounds.maximum_workers().to_be_bytes());
    out.extend_from_slice(&bounds.connection_bytes().to_be_bytes());
    out.extend_from_slice(&bounds.total_bytes().to_be_bytes());
    out.extend_from_slice(&bounds.connection_millis().to_be_bytes());
    out.extend_from_slice(&bounds.total_millis().to_be_bytes());
    out.extend_from_slice(&bounds.observations().to_be_bytes());
    out.extend_from_slice(&bounds.header_bytes().to_be_bytes());
    put_u32(&mut out, options.credentials().len())?;
    for reference in options.credentials() {
        encode_reference(&mut out, *reference);
    }
    if out.len() > MAX_CANONICAL_BYTES {
        return Err(crate::error::invalid("canonical network plan exceeds its byte bound"));
    }
    Ok(out)
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

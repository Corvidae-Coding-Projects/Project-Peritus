//! Deterministic GUID projection for one dynamic WFP session.

use std::net::IpAddr;

use windows_sys::core::GUID;

use crate::ProxyRoute;

pub(super) struct PolicyKeys {
    pub(super) session: GUID,
    pub(super) sublayer: GUID,
    pub(super) allow_v4: GUID,
    pub(super) block_v4: GUID,
    pub(super) block_v6: GUID,
}

impl PolicyKeys {
    pub(super) fn for_route(sid: &str, route: ProxyRoute) -> Self {
        let mut seed = Vec::from(b"PERITUS-WINDOWS-WFP-POLICY-V1\0".as_slice());
        seed.extend_from_slice(sid.as_bytes());
        seed.extend_from_slice(route.network_plan_digest().as_bytes());
        seed.extend_from_slice(route.filter_digest().as_bytes());
        match route.endpoint().ip() {
            IpAddr::V4(value) => seed.extend_from_slice(&value.octets()),
            IpAddr::V6(value) => seed.extend_from_slice(&value.octets()),
        }
        seed.extend_from_slice(&route.endpoint().port().to_be_bytes());
        Self::from_seed(&seed)
    }

    pub(super) fn for_probe(sid: &str, identity: peritus_types::Sha256Digest) -> Self {
        let mut seed = Vec::from(b"PERITUS-WINDOWS-WFP-PROBE-V1\0".as_slice());
        seed.extend_from_slice(&std::process::id().to_be_bytes());
        seed.extend_from_slice(sid.as_bytes());
        seed.extend_from_slice(identity.as_bytes());
        Self::from_seed(&seed)
    }

    fn from_seed(seed: &[u8]) -> Self {
        Self {
            session: derived_guid(seed, b"session"),
            sublayer: derived_guid(seed, b"sublayer"),
            allow_v4: derived_guid(seed, b"allow-v4"),
            block_v4: derived_guid(seed, b"block-v4"),
            block_v6: derived_guid(seed, b"block-v6"),
        }
    }
}

fn derived_guid(seed: &[u8], label: &[u8]) -> GUID {
    let mut input = Vec::with_capacity(seed.len() + label.len() + 1);
    input.extend_from_slice(seed);
    input.push(0);
    input.extend_from_slice(label);
    let digest = peritus_codec::sha256(&input);
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest.as_bytes()[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    GUID::from_u128(u128::from_be_bytes(bytes))
}

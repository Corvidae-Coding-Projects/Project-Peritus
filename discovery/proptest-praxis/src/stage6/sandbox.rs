//! Independent logical capability oracles for inert sandbox contracts.

use crate::Metrics;
use peritus_sandbox::{
    DnsName, FileDecision, FileOperation, FileOperationSet, FilesystemContract, FilesystemRule,
    HostMatcher, NetworkContract, NetworkDecision, NetworkHost, NetworkRule, NetworkTarget,
    PathScope, PortRange, RuleEffect, SandboxPath, Transport,
};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

const NAMES: [&str; 6] =
    ["example.com", "a.example.com", "badexample.com", "other.net", "a.other.net", "EXAMPLE.COM."];
const PATHS: [&str; 10] =
    ["/", "/a", "/a/b", "/a-b", "/a//b", "/α", "/α/β", "C:/A", "C:/A/x", "C:/A-b"];
const PORTS: [u16; 6] = [1, 22, 80, 443, 8080, 65535];
const OPS: [FileOperation; 7] = [
    FileOperation::Discover,
    FileOperation::Metadata,
    FileOperation::Read,
    FileOperation::Execute,
    FileOperation::Create,
    FileOperation::Write,
    FileOperation::Remove,
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Case {
    Network { rules: Vec<NetworkRuleSpec> },
    Filesystem { rules: Vec<FileRuleSpec> },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkRuleSpec {
    pub deny: bool,
    pub selector: NetworkSelector,
    pub udp: bool,
    pub first: u8,
    pub last: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum NetworkSelector {
    DnsExact(u8),
    DnsSuffix(u8),
    V4 { address: [u8; 4], prefix: u8 },
    V6 { address: [u8; 16], prefix: u8 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FileRuleSpec {
    pub deny: bool,
    pub path: u8,
    pub descendants: bool,
    pub operations: u8,
}

fn bump(metrics: &mut Metrics, key: &str) {
    *metrics.entry(key.into()).or_default() += 1;
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let selector = prop_oneof![
        3 => (0_u8..6).prop_map(NetworkSelector::DnsExact),
        3 => (0_u8..6).prop_map(NetworkSelector::DnsSuffix),
        2 => (any::<[u8; 4]>(), 0_u8..=32).prop_map(|(address, prefix)| NetworkSelector::V4 { address, prefix }),
        2 => (any::<[u8; 16]>(), 0_u8..=128).prop_map(|(address, prefix)| NetworkSelector::V6 { address, prefix }),
    ];
    let network_rule = (any::<bool>(), selector, any::<bool>(), 0_u8..6, 0_u8..6).prop_map(
        |(deny, selector, udp, first, last)| NetworkRuleSpec { deny, selector, udp, first, last },
    );
    let file_rule = (any::<bool>(), 0_u8..10, any::<bool>(), 1_u8..=127).prop_map(
        |(deny, path, descendants, operations)| FileRuleSpec {
            deny,
            path,
            descendants,
            operations,
        },
    );
    let max_rules = max_steps.clamp(1, 32).min(12);
    let broad = prop_oneof![
        prop::collection::vec(network_rule.clone(), 0..=max_rules)
            .prop_map(|rules| Case::Network { rules }),
        prop::collection::vec(file_rule, 0..=max_rules)
            .prop_map(|rules| Case::Filesystem { rules }),
    ];
    let deny_overlap = network_rule.prop_map(|mut extra| {
        let allow = NetworkRuleSpec {
            deny: false,
            selector: NetworkSelector::DnsSuffix(0),
            udp: false,
            first: 2,
            last: 3,
        };
        let deny = NetworkRuleSpec {
            deny: true,
            selector: NetworkSelector::DnsExact(1),
            udp: false,
            first: 3,
            last: 3,
        };
        extra.first = extra.first.min(5);
        Case::Network { rules: vec![allow, deny, extra] }
    });
    prop_oneof![5 => broad, 2 => deny_overlap].boxed()
}

fn canonical_name(index: u8) -> Result<String, String> {
    let text = NAMES.get(usize::from(index)).ok_or("invalid DNS selector index")?;
    Ok(text.trim_end_matches('.').to_ascii_lowercase())
}

fn rule_effect(deny: bool) -> RuleEffect {
    if deny { RuleEffect::Deny } else { RuleEffect::Allow }
}

fn network_selector(spec: &NetworkSelector) -> Result<HostMatcher, String> {
    Ok(match spec {
        NetworkSelector::DnsExact(index) => HostMatcher::DnsExact(
            DnsName::new(NAMES.get(usize::from(*index)).ok_or("invalid DNS index")?.to_string())
                .map_err(|e| format!("DNS exact: {e:?}"))?,
        ),
        NetworkSelector::DnsSuffix(index) => HostMatcher::DnsSuffix(
            DnsName::new(NAMES.get(usize::from(*index)).ok_or("invalid DNS index")?.to_string())
                .map_err(|e| format!("DNS suffix: {e:?}"))?,
        ),
        NetworkSelector::V4 { address, prefix } => {
            HostMatcher::ip_prefix(IpAddr::V4(Ipv4Addr::from(*address)), *prefix)
                .map_err(|e| format!("IPv4 prefix: {e:?}"))?
        }
        NetworkSelector::V6 { address, prefix } => {
            HostMatcher::ip_prefix(IpAddr::V6(Ipv6Addr::from(*address)), *prefix)
                .map_err(|e| format!("IPv6 prefix: {e:?}"))?
        }
    })
}

fn prefix_matches(expected: &[u8], actual: &[u8], prefix: u8) -> bool {
    // Component-by-component bit comparison, independent of production's u32/u128 masks.
    (0..usize::from(prefix)).all(|bit| {
        let mask = 1_u8 << (7 - bit % 8);
        expected[bit / 8] & mask == actual[bit / 8] & mask
    })
}

fn selector_matches(selector: &NetworkSelector, host: &NetworkHost) -> Result<bool, String> {
    Ok(match (selector, host) {
        (NetworkSelector::DnsExact(index), NetworkHost::Dns(actual)) => {
            canonical_name(*index)? == actual.as_str()
        }
        (NetworkSelector::DnsSuffix(index), NetworkHost::Dns(actual)) => {
            let expected = canonical_name(*index)?;
            actual.as_str() == expected
                || actual.as_str().strip_suffix(&format!(".{expected}")).is_some()
        }
        (NetworkSelector::V4 { address, prefix }, NetworkHost::Ip(IpAddr::V4(actual))) => {
            prefix_matches(address, &actual.octets(), *prefix)
        }
        (NetworkSelector::V6 { address, prefix }, NetworkHost::Ip(IpAddr::V6(actual))) => {
            prefix_matches(address, &actual.octets(), *prefix)
        }
        _ => false,
    })
}

fn network(case: &Case, metrics: &mut Metrics) -> Result<(), String> {
    let Case::Network { rules } = case else { return Err("network case mismatch".into()) };
    if rules.len() > 32 {
        return Err("network fixture exceeds bound".into());
    }
    let mut production_rules = Vec::new();
    for rule in rules {
        let (first, last) = (rule.first.min(rule.last), rule.first.max(rule.last));
        let ports = PortRange::new(PORTS[usize::from(first)], PORTS[usize::from(last)])
            .map_err(|e| format!("port range: {e:?}"))?;
        production_rules.push(NetworkRule::new(
            rule_effect(rule.deny),
            network_selector(&rule.selector)?,
            if rule.udp { Transport::Udp } else { Transport::Tcp },
            ports,
        ));
    }
    let contract =
        NetworkContract::new(production_rules).map_err(|e| format!("network contract: {e:?}"))?;
    let mut hosts: Vec<NetworkHost> = NAMES
        .iter()
        .map(|name| {
            DnsName::new(*name).map(NetworkHost::Dns).map_err(|e| format!("query DNS: {e:?}"))
        })
        .collect::<Result<_, _>>()?;
    for octets in
        [[0, 0, 0, 0], [127, 0, 0, 1], [169, 254, 169, 254], [192, 168, 1, 1], [8, 8, 8, 8]]
    {
        hosts.push(NetworkHost::Ip(IpAddr::V4(Ipv4Addr::from(octets))));
    }
    hosts.push(NetworkHost::Ip(IpAddr::V6(Ipv6Addr::LOCALHOST)));
    for rule in rules {
        match &rule.selector {
            NetworkSelector::V4 { address, .. } => {
                hosts.push(NetworkHost::Ip(IpAddr::V4(Ipv4Addr::from(*address))));
                let mut adjacent = *address;
                adjacent[3] ^= 1;
                hosts.push(NetworkHost::Ip(IpAddr::V4(Ipv4Addr::from(adjacent))));
            }
            NetworkSelector::V6 { address, .. } => {
                hosts.push(NetworkHost::Ip(IpAddr::V6(Ipv6Addr::from(*address))));
                let mut adjacent = *address;
                adjacent[15] ^= 1;
                hosts.push(NetworkHost::Ip(IpAddr::V6(Ipv6Addr::from(adjacent))));
            }
            _ => {}
        }
    }
    for host in hosts {
        for transport in [Transport::Tcp, Transport::Udp] {
            for port in PORTS {
                let mut allow = false;
                let mut deny = false;
                for rule in rules {
                    let (first, last) = (rule.first.min(rule.last), rule.first.max(rule.last));
                    if (if rule.udp { Transport::Udp } else { Transport::Tcp }) == transport
                        && port >= PORTS[usize::from(first)]
                        && port <= PORTS[usize::from(last)]
                        && selector_matches(&rule.selector, &host)?
                    {
                        if rule.deny {
                            deny = true;
                        } else {
                            allow = true;
                        }
                    }
                }
                let expected = if deny {
                    NetworkDecision::DeniedByRule
                } else if allow {
                    NetworkDecision::Allowed
                } else {
                    NetworkDecision::DeniedByDefault
                };
                let target = NetworkTarget::new(host.clone(), transport, port)
                    .map_err(|e| format!("query target: {e:?}"))?;
                let observed = contract.decide(&target);
                if observed != expected {
                    return Err(format!(
                        "network mismatch for {host:?}/{transport:?}/{port}: expected {expected:?}, observed {observed:?}"
                    ));
                }
                bump(
                    metrics,
                    match expected {
                        NetworkDecision::Allowed => "network_allowed",
                        NetworkDecision::DeniedByRule => "network_deny_rule",
                        NetworkDecision::DeniedByDefault => "network_deny_default",
                    },
                );
            }
        }
    }
    Ok(())
}

fn normalized_path(index: u8) -> Result<String, String> {
    let path = *PATHS.get(usize::from(index)).ok_or("invalid path index")?;
    Ok(if path == "/" { "/".into() } else { path.trim_end_matches('/').into() })
}

fn file_matches(rule_path: &str, candidate: &str, descendants: bool) -> bool {
    if rule_path == candidate {
        return true;
    }
    if !descendants {
        return false;
    }
    if rule_path == "/" {
        return candidate.starts_with('/');
    }
    // Compare complete logical components; a sibling with a shared byte prefix is distinct.
    let rule_components: Vec<_> = rule_path.split('/').collect();
    let candidate_components: Vec<_> = candidate.split('/').collect();
    candidate_components.starts_with(&rule_components)
}

fn filesystem(case: &Case, metrics: &mut Metrics) -> Result<(), String> {
    let Case::Filesystem { rules } = case else { return Err("filesystem case mismatch".into()) };
    if rules.len() > 32 {
        return Err("filesystem fixture exceeds bound".into());
    }
    let mut production_rules = Vec::new();
    for rule in rules {
        let path = SandboxPath::new(normalized_path(rule.path)?)
            .map_err(|e| format!("rule path: {e:?}"))?;
        let operations = FileOperationSet::from_operations(
            OPS.into_iter()
                .enumerate()
                .filter(|(index, _)| rule.operations & (1 << index) != 0)
                .map(|(_, operation)| operation),
        );
        production_rules.push(
            FilesystemRule::new(
                rule_effect(rule.deny),
                path,
                if rule.descendants { PathScope::Descendants } else { PathScope::Exact },
                operations,
            )
            .map_err(|e| format!("file rule: {e:?}"))?,
        );
    }
    let contract = FilesystemContract::new(production_rules)
        .map_err(|e| format!("filesystem contract: {e:?}"))?;
    for (path_index, raw_path) in PATHS.iter().enumerate() {
        let candidate = SandboxPath::new(*raw_path).map_err(|e| format!("query path: {e:?}"))?;
        for (operation_index, operation) in OPS.into_iter().enumerate() {
            let mut allow = false;
            let mut deny = false;
            for rule in rules {
                if rule.operations & (1 << operation_index) == 0 {
                    continue;
                }
                let rule_path = normalized_path(rule.path)?;
                if file_matches(&rule_path, candidate.as_str(), rule.descendants) {
                    if rule.deny {
                        deny = true;
                    } else {
                        allow = true;
                    }
                }
            }
            let expected = if deny {
                FileDecision::DeniedByRule
            } else if allow {
                FileDecision::Allowed
            } else {
                FileDecision::DeniedByDefault
            };
            let observed = contract.decide(&candidate, operation);
            if observed != expected {
                return Err(format!(
                    "filesystem path {path_index}/{operation_index}: expected {expected:?}, observed {observed:?}"
                ));
            }
            bump(
                metrics,
                match expected {
                    FileDecision::Allowed => "file_allowed",
                    FileDecision::DeniedByRule => "file_deny_rule",
                    FileDecision::DeniedByDefault => "file_deny_default",
                },
            );
        }
    }
    Ok(())
}

pub fn check(case: &Case) -> Result<Metrics, String> {
    let mut metrics = Metrics::new();
    match case {
        Case::Network { .. } => network(case, &mut metrics)?,
        Case::Filesystem { .. } => filesystem(case, &mut metrics)?,
    }
    Ok(metrics)
}

pub fn examples() -> Vec<Case> {
    vec![
        Case::Network {
            rules: vec![
                NetworkRuleSpec {
                    deny: false,
                    selector: NetworkSelector::DnsSuffix(0),
                    udp: false,
                    first: 2,
                    last: 3,
                },
                NetworkRuleSpec {
                    deny: true,
                    selector: NetworkSelector::DnsExact(1),
                    udp: false,
                    first: 3,
                    last: 3,
                },
            ],
        },
        Case::Network {
            rules: vec![
                NetworkRuleSpec {
                    deny: false,
                    selector: NetworkSelector::V4 { address: [127, 0, 0, 1], prefix: 8 },
                    udp: false,
                    first: 0,
                    last: 5,
                },
                NetworkRuleSpec {
                    deny: true,
                    selector: NetworkSelector::V4 { address: [127, 0, 0, 1], prefix: 32 },
                    udp: false,
                    first: 3,
                    last: 3,
                },
            ],
        },
        Case::Network {
            rules: vec![NetworkRuleSpec {
                deny: false,
                selector: NetworkSelector::V6 { address: [0; 16], prefix: 0 },
                udp: true,
                first: 0,
                last: 5,
            }],
        },
        Case::Filesystem {
            rules: vec![
                FileRuleSpec { deny: false, path: 1, descendants: true, operations: 127 },
                FileRuleSpec { deny: true, path: 2, descendants: false, operations: 1 << 2 },
            ],
        },
        Case::Filesystem {
            rules: vec![
                FileRuleSpec { deny: false, path: 7, descendants: true, operations: 127 },
                FileRuleSpec { deny: true, path: 9, descendants: false, operations: 1 << 5 },
            ],
        },
    ]
}

pub fn calibration() -> Case {
    Case::Network {
        rules: vec![
            NetworkRuleSpec {
                deny: false,
                selector: NetworkSelector::DnsSuffix(0),
                udp: false,
                first: 2,
                last: 3,
            },
            NetworkRuleSpec {
                deny: true,
                selector: NetworkSelector::DnsExact(1),
                udp: false,
                first: 3,
                last: 3,
            },
        ],
    }
}

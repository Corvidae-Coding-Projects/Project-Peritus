//! Synthetic host-boundary histories: exact secret leases and Unix endpoint paths.

use crate::Metrics;
use peritus_local_socket::bounded_path;
use peritus_sandbox::{
    BrokeredHandleLabel, EnvironmentName, SandboxPath, SecretDelivery, SecretReference,
};
use peritus_secrets::{SecretLease, SecretLeaseId, SecretLeaseState};
use peritus_types::{EnvironmentId, ProcessId, ResourceId, Sha256Digest};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Case {
    Lease { uses: u8, expires: u16, operations: Vec<LeaseOperation> },
    Socket { original: String, maximum: u8 },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LeaseOperation {
    Use { binding: u8, at: u16 },
    Revoke,
    Observe,
}

fn bump(metrics: &mut Metrics, key: &str) {
    *metrics.entry(key.into()).or_default() += 1;
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let operation = prop_oneof![
        8 => (0_u8..=9, 0_u16..=200).prop_map(|(binding, at)| LeaseOperation::Use { binding, at }),
        2 => Just(LeaseOperation::Revoke),
        1 => Just(LeaseOperation::Observe),
    ];
    let general =
        (1_u8..=8, 1_u16..=100, prop::collection::vec(operation.clone(), 0..=max_steps.min(64)))
            .prop_map(|(uses, expires, operations)| Case::Lease { uses, expires, operations });
    let exhaustion = (
        1_u8..=4,
        1_u16..=100,
        prop::collection::vec(operation, 0..=max_steps.min(64).saturating_sub(4)),
    )
        .prop_map(|(uses, expires, mut suffix)| {
            let mut operations = vec![
                LeaseOperation::Use { binding: 0, at: 0 },
                LeaseOperation::Use { binding: 2, at: 0 },
                LeaseOperation::Use { binding: 0, at: expires },
                LeaseOperation::Observe,
            ];
            operations.append(&mut suffix);
            Case::Lease { uses, expires, operations }
        });
    let socket = (0_u16..=180, 0_u8..=130, 0_u8..=6).prop_map(|(length, maximum, form)| {
        let tail = "x".repeat(usize::from(length));
        let original = match form {
            0 => format!("/tmp/{tail}"),
            1 => format!("/tmp/α/{tail}"),
            2 => format!("/tmp/a/{tail}/sock"),
            3 => format!("relative/{tail}"),
            4 => format!("/tmp/{tail}/."),
            5 => format!("/tmp/{tail}/.."),
            _ => format!("/tmp/{tail}/\0sock"),
        };
        Case::Socket { original, maximum }
    });
    // Keep the lease live through every binding check before exhausting it.
    // Random histories usually expire/revoke before reaching most drift dimensions.
    let binding_probe = (1_u8..=8, 1_u16..=100).prop_map(|(uses, expires)| {
        let mut operations = (1..=9)
            .map(|binding| LeaseOperation::Use { binding, at: expires - 1 })
            .collect::<Vec<_>>();
        operations.extend((0..=uses).map(|_| LeaseOperation::Use { binding: 0, at: expires - 1 }));
        operations.push(LeaseOperation::Revoke);
        operations.push(LeaseOperation::Observe);
        Case::Lease { uses, expires, operations }
    });
    prop_oneof![5 => general, 3 => exhaustion, 3 => binding_probe, 3 => socket].boxed()
}

fn id(byte: u8) -> [u8; 16] {
    [byte; 16]
}

fn lease(case: &Case, metrics: &mut Metrics) -> Result<(), String> {
    let Case::Lease { uses, expires, operations } = case else {
        return Err("lease case mismatch".into());
    };
    if *uses == 0 || *expires == 0 || operations.len() > 128 {
        return Err("invalid lease fixture".into());
    }
    let owner = ProcessId::new(id(1)).map_err(|e| format!("owner: {e:?}"))?;
    let environment = EnvironmentId::new(id(2)).map_err(|e| format!("environment: {e:?}"))?;
    let reference = SecretReference::new(
        ResourceId::new(id(3)).map_err(|e| format!("resource: {e:?}"))?,
        Sha256Digest::new([4; 32]),
    );
    let delivery = SecretDelivery::Environment(
        EnvironmentName::new("PRACTICE_SECRET").map_err(|e| format!("delivery: {e:?}"))?,
    );
    let sandbox = Sha256Digest::new([5; 32]);
    let execution = Sha256Digest::new([6; 32]);
    let mut actual = SecretLease::new(
        SecretLeaseId::new(id(7)),
        owner,
        environment,
        sandbox,
        execution,
        reference,
        delivery.clone(),
        u32::from(*uses),
        u64::from(*expires),
    )
    .map_err(|e| format!("lease construction: {e:?}"))?;
    let mut state = SecretLeaseState::Active;
    let mut remaining = u32::from(*uses);
    for (index, operation) in operations.iter().enumerate() {
        match operation {
            LeaseOperation::Revoke => {
                actual.revoke();
                if state == SecretLeaseState::Active {
                    state = SecretLeaseState::Revoked;
                }
                bump(metrics, "lease_revocations");
            }
            LeaseOperation::Observe => bump(metrics, "lease_observations"),
            LeaseOperation::Use { binding, at } => {
                let chosen_owner = if *binding == 1 {
                    ProcessId::new(id(11)).map_err(|e| format!("owner: {e:?}"))?
                } else {
                    owner
                };
                let chosen_environment = if *binding == 2 {
                    EnvironmentId::new(id(12)).map_err(|e| format!("environment: {e:?}"))?
                } else {
                    environment
                };
                let chosen_sandbox =
                    if *binding == 3 { Sha256Digest::new([13; 32]) } else { sandbox };
                let chosen_execution =
                    if *binding == 4 { Sha256Digest::new([14; 32]) } else { execution };
                let chosen_reference = match *binding {
                    5 => SecretReference::new(reference.resource_id(), Sha256Digest::new([15; 32])),
                    7 => SecretReference::new(
                        ResourceId::new(id(16)).map_err(|e| format!("resource: {e:?}"))?,
                        reference.version(),
                    ),
                    _ => reference,
                };
                let chosen_delivery = match *binding {
                    6 => SecretDelivery::Environment(
                        EnvironmentName::new("OTHER_SECRET")
                            .map_err(|e| format!("delivery: {e:?}"))?,
                    ),
                    8 => SecretDelivery::File(
                        SandboxPath::new("/synthetic/secret".to_owned())
                            .map_err(|e| format!("delivery: {e:?}"))?,
                    ),
                    9 => SecretDelivery::BrokeredHandle(
                        BrokeredHandleLabel::new("synthetic-secret".to_owned())
                            .map_err(|e| format!("delivery: {e:?}"))?,
                    ),
                    _ => delivery.clone(),
                };
                // The reference model uses a closed lifecycle table, independent of the
                // production lease's condition chain. Expiry is half-open and observed before
                // any binding check; a binding rejection leaves a live lease unchanged.
                let expected_ok = if state != SecretLeaseState::Active {
                    false
                } else if *at >= *expires {
                    state = SecretLeaseState::Expired;
                    false
                } else if *binding != 0 {
                    false
                } else {
                    remaining -= 1;
                    if remaining == 0 {
                        state = SecretLeaseState::Exhausted;
                    }
                    true
                };
                let observed = actual.consume(
                    chosen_owner,
                    chosen_environment,
                    chosen_sandbox,
                    chosen_execution,
                    chosen_reference,
                    &chosen_delivery,
                    u64::from(*at),
                );
                if observed.is_ok() != expected_ok {
                    return Err(format!(
                        "lease operation {index}: expected success {expected_ok}, got {observed:?}"
                    ));
                }
                bump(metrics, if expected_ok { "lease_accepted" } else { "lease_rejected" });
                if *at == *expires {
                    bump(metrics, "lease_exact_expiry_attempts");
                }
                if *binding != 0 {
                    bump(metrics, "lease_binding_drift_attempts");
                    if state == SecretLeaseState::Active && *at < *expires {
                        bump(metrics, &format!("live_binding_drift_{binding}"));
                    }
                }
            }
        }
        if actual.state() != state || actual.remaining_uses() != remaining {
            return Err(format!(
                "lease operation {index}: expected {state:?}/{remaining}, observed {:?}/{}",
                actual.state(),
                actual.remaining_uses()
            ));
        }
    }
    Ok(())
}

fn socket(case: &Case, metrics: &mut Metrics) -> Result<(), String> {
    let Case::Socket { original, maximum } = case else {
        return Err("socket case mismatch".into());
    };
    if original.len() > 256 {
        return Err("socket fixture too large".into());
    }
    let path = Path::new(original);
    let bytes = original.as_bytes();
    let final_name = bytes.rsplit(|byte| *byte == b'/').next().unwrap_or_default();
    let valid =
        bytes.starts_with(b"/") && !bytes.contains(&0) && !matches!(final_name, b"" | b"." | b"..");
    let fallback_len = "/tmp/peritus-".len() + 32 + "/daemon.sock".len();
    let max = usize::from(*maximum);
    let expected_ok = valid && (bytes.len() <= max || max >= fallback_len);
    let first = bounded_path(path, max);
    let second = bounded_path(path, max);
    if first.is_ok() != expected_ok || second.is_ok() != expected_ok {
        return Err(format!(
            "socket path expected success {expected_ok}, observed {first:?}/{second:?}"
        ));
    }
    if let (Ok(first), Ok(second)) = (first, second) {
        if first != second || first.as_os_str().as_encoded_bytes().len() > max {
            return Err("bounded socket path is unstable or exceeds byte limit".into());
        }
        if bytes.len() <= max {
            if first != path {
                return Err("short Unix endpoint path was changed".into());
            }
            bump(metrics, "socket_native_paths");
        } else {
            if !first.to_string_lossy().starts_with("/tmp/peritus-")
                || !first.to_string_lossy().ends_with("/daemon.sock")
            {
                return Err("long Unix endpoint did not use private runtime shape".into());
            }
            // Identity must include both the parent path and the final component.
            // A constant/final-name-only hash passed the previous stability oracle.
            for alternate in [format!("/other-root{original}"), format!("{original}a")] {
                let alternate = bounded_path(Path::new(&alternate), max)
                    .map_err(|error| format!("alternate long socket path rejected: {error}"))?;
                if alternate == first {
                    return Err(
                        "distinct endpoint identities collapsed to one socket address".into()
                    );
                }
                bump(metrics, "socket_identity_separation_checks");
            }
            bump(metrics, "socket_bounded_paths");
        }
    } else {
        bump(metrics, "socket_rejections");
    }
    Ok(())
}

pub fn check(case: &Case) -> Result<Metrics, String> {
    let mut metrics = Metrics::new();
    match case {
        Case::Lease { .. } => lease(case, &mut metrics)?,
        Case::Socket { .. } => socket(case, &mut metrics)?,
    }
    Ok(metrics)
}

pub fn examples() -> Vec<Case> {
    vec![
        Case::Lease {
            uses: 1,
            expires: 10,
            operations: vec![
                LeaseOperation::Use { binding: 2, at: 9 },
                LeaseOperation::Use { binding: 0, at: 10 },
                LeaseOperation::Use { binding: 0, at: 0 },
            ],
        },
        Case::Lease {
            uses: 2,
            expires: 10,
            operations: vec![
                LeaseOperation::Use { binding: 0, at: 0 },
                LeaseOperation::Use { binding: 0, at: 9 },
                LeaseOperation::Use { binding: 0, at: 9 },
                LeaseOperation::Revoke,
            ],
        },
        Case::Lease {
            uses: 2,
            expires: 10,
            operations: vec![LeaseOperation::Revoke, LeaseOperation::Use { binding: 0, at: 1 }],
        },
        Case::Lease {
            uses: 1,
            expires: 10,
            operations: (1..=9)
                .map(|binding| LeaseOperation::Use { binding, at: 9 })
                .chain([
                    LeaseOperation::Use { binding: 0, at: 9 },
                    LeaseOperation::Use { binding: 0, at: 9 },
                ])
                .collect(),
        },
        Case::Socket { original: "/tmp/short.sock".into(), maximum: 107 },
        Case::Socket { original: format!("/tmp/{}/daemon.sock", "a".repeat(150)), maximum: 107 },
        Case::Socket { original: "/tmp/x/..".into(), maximum: 107 },
        Case::Socket { original: "relative.sock".into(), maximum: 107 },
    ]
}

pub fn calibration() -> Case {
    Case::Lease {
        uses: 1,
        expires: 10,
        operations: vec![LeaseOperation::Use { binding: 0, at: 10 }],
    }
}

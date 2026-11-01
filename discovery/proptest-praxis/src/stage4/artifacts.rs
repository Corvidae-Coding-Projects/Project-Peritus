//! Content/reference ledger, independent GC table, and actual SQLite/filesystem recovery.
use crate::Metrics;
use peritus_artifact_store::{
    ArtifactDigest, ArtifactReferenceSet, ArtifactStore, CollectionGeneration, EncryptionMetadata,
    GcAction, GcInventoryEntry, GcPlan, IntegrityState, MediaType, Publication, QuarantineState,
    QuotaPlan, QuotaSnapshot, ReferenceOwner, ReferenceRoots, StoreConfig, WriteRequest,
};
use peritus_types::{EventId, Sha256Digest};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Case {
    Gc { generation: u64, rows: Vec<Row>, journal: Vec<u8>, evidence: Vec<u8> },
    Quota { used: u64, reserved: u64, limit: Option<u64>, added: u64 },
    History { salt: u8, limited: bool, operations: Vec<Operation> },
    Recovery { cut: u8, payload: Vec<u8> },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Row {
    key: u8,
    size: u64,
    since: Option<u64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    Put { key: u8, style: u8, owned: bool },
    Reference { key: u8, owner: u8, add: bool },
    Plan { slot: u8, advance: bool },
    Apply(u8),
    Reopen,
    Read(u8),
}
fn bump(m: &mut Metrics, k: &str) {
    *m.entry(k.into()).or_default() += 1;
}
fn scalar() -> BoxedStrategy<u64> {
    prop_oneof![5=>0_u64..256,1=>Just(u64::MAX),1=>Just(u64::MAX-1),1=>Just(i64::MAX as u64)]
        .boxed()
}
pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let row = (0_u8..6, scalar(), prop::option::of(0_u64..8)).prop_map(|(key, size, since)| Row {
        key,
        size,
        since,
    });
    let gc = (
        0_u64..8,
        prop::collection::vec(row, 0..8),
        prop::collection::vec(0_u8..8, 0..5),
        prop::collection::vec(0_u8..8, 0..5),
    )
        .prop_map(|(generation, rows, journal, evidence)| Case::Gc {
            generation,
            rows,
            journal,
            evidence,
        });
    let quota = (scalar(), scalar(), prop::option::of(scalar()), scalar())
        .prop_map(|(used, reserved, limit, added)| Case::Quota { used, reserved, limit, added });
    let operation = prop_oneof![
        7=>(0_u8..6,0_u8..5,any::<bool>()).prop_map(|(key,style,owned)|Operation::Put{key,style,owned}),
        4=>(0_u8..6,0_u8..4,any::<bool>()).prop_map(|(key,owner,add)|Operation::Reference{key,owner,add}),
        3=>(0_u8..3,any::<bool>()).prop_map(|(slot,advance)|Operation::Plan{slot,advance}),
        3=>(0_u8..3).prop_map(Operation::Apply),
        2=>Just(Operation::Reopen),
        2=>(0_u8..6).prop_map(Operation::Read),
    ];
    let histories =
        (0_u8..8, any::<bool>(), prop::collection::vec(operation.clone(), 0..=max_steps.min(64)))
            .prop_map(|(salt, limited, operations)| Case::History { salt, limited, operations });
    let interacting = (
        0_u8..8,
        any::<bool>(),
        prop::collection::vec(operation, 0..=max_steps.saturating_sub(9).min(32)),
    )
        .prop_map(move |(salt, limited, mut suffix)| {
            let mut operations = vec![
                Operation::Put { key: 0, style: 0, owned: true },
                Operation::Reference { key: 0, owner: 0, add: true },
                Operation::Reference { key: 0, owner: 0, add: false },
                Operation::Plan { slot: 0, advance: true },
                Operation::Reference { key: 0, owner: 1, add: true },
                Operation::Apply(0),
                Operation::Reference { key: 0, owner: 1, add: false },
                Operation::Apply(0),
                Operation::Reopen,
            ];
            operations.append(&mut suffix);
            operations.truncate(max_steps.min(64));
            Case::History { salt, limited, operations }
        });
    let generation_edges = (1_u64..8, 0_u8..8, 0_u64..64, any::<bool>(), any::<bool>()).prop_map(
        |(generation, key, size, marked, future)| Case::Gc {
            generation,
            rows: vec![Row { key, size, since: Some(generation + u64::from(future)) }],
            journal: if marked { vec![key] } else { vec![] },
            evidence: vec![],
        },
    );
    prop_oneof![3=>gc,3=>generation_edges,2=>quota,4=>histories,2=>interacting,2=>(0_u8..4,prop::collection::vec(any::<u8>(),1..64)).prop_map(|(cut,payload)|Case::Recovery{cut,payload})].boxed()
}
fn synthetic(key: u8) -> ArtifactDigest {
    ArtifactDigest::new([key; 32])
}
fn gc(case: &Case, metrics: &mut Metrics) -> Result<(), String> {
    let Case::Gc { generation, rows, journal, evidence } = case else {
        return Err("GC variant mismatch".into());
    };
    if rows.len() > 64 || journal.len() > 64 || evidence.len() > 64 {
        return Err("GC bounds exceeded".into());
    }
    let keys: BTreeSet<_> = rows.iter().map(|r| r.key).collect();
    let roots: BTreeSet<_> = journal.iter().chain(evidence).copied().collect();
    let valid = *generation > 0
        && keys.len() == rows.len()
        && roots.is_subset(&keys)
        && rows.iter().all(|r| r.since.is_none_or(|s| s > 0 && s <= *generation));
    let actual = (|| {
        let generation = CollectionGeneration::new(*generation)?;
        let inventory = rows
            .iter()
            .map(|row| {
                Ok(GcInventoryEntry::new(
                    synthetic(row.key),
                    row.size,
                    match row.since {
                        None => QuarantineState::Active,
                        Some(since) => QuarantineState::Quarantined {
                            since: CollectionGeneration::new(since)?,
                        },
                    },
                ))
            })
            .collect::<Result<Vec<_>, peritus_artifact_store::ArtifactStoreError>>()?;
        GcPlan::build(
            generation,
            inventory,
            &ReferenceRoots::new(
                journal.iter().map(|k| synthetic(*k)).collect(),
                evidence.iter().map(|k| synthetic(*k)).collect(),
            ),
        )
    })();
    if actual.is_ok() != valid {
        return Err(format!("GC acceptance differs; expected {valid}, actual {actual:?}"));
    }
    if let Ok(plan) = actual {
        let mut expected = Vec::new();
        let mut sorted = rows.clone();
        sorted.sort_by_key(|r| r.key);
        for row in sorted {
            let action = match (row.since, roots.contains(&row.key)) {
                (None, false) => Some((0, synthetic(row.key), row.size, *generation)),
                (Some(since), true) => Some((1, synthetic(row.key), row.size, since)),
                (Some(since), false) if since < *generation => {
                    Some((2, synthetic(row.key), row.size, since))
                }
                _ => None,
            };
            if let Some(action) = action {
                expected.push(action);
            }
        }
        let actual = plan
            .actions()
            .iter()
            .map(|action| match *action {
                GcAction::Quarantine { digest, size, generation } => {
                    (0, digest, size, generation.get())
                }
                GcAction::Restore { digest, size, since } => (1, digest, size, since.get()),
                GcAction::Delete { digest, size, since } => (2, digest, size, since.get()),
            })
            .collect::<Vec<_>>();
        let marked: BTreeSet<_> = plan.marked().iter().copied().collect();
        if actual != expected || marked != roots.iter().map(|key| synthetic(*key)).collect() {
            return Err("GC exact action/mark table differs".into());
        }
        bump(metrics, "valid_gc_plans");
        for (kind, _, _, _) in expected {
            bump(
                metrics,
                match kind {
                    0 => "planned_quarantines",
                    1 => "planned_restores",
                    _ => "planned_deletions",
                },
            );
        }
    } else {
        bump(metrics, "rejected_gc_plans");
    }
    Ok(())
}
fn quota(
    used: u64,
    reserved: u64,
    limit: Option<u64>,
    added: u64,
    m: &mut Metrics,
) -> Result<(), String> {
    let before = u128::from(used) + u128::from(reserved);
    let snapshot_valid = limit != Some(0)
        && before <= u128::from(u64::MAX)
        && limit.is_none_or(|limit| before <= u128::from(limit));
    let actual = match limit {
        Some(limit) => QuotaSnapshot::new(used, reserved, limit),
        None => QuotaSnapshot::without_limit(used, reserved),
    };
    if actual.is_ok() != snapshot_valid {
        return Err("quota snapshot acceptance differs from wider arithmetic".into());
    }
    if let Ok(snapshot) = actual {
        let total = before + u128::from(added);
        let valid =
            total <= u128::from(u64::MAX) && limit.is_none_or(|limit| total <= u128::from(limit));
        let plan = QuotaPlan::reserve(snapshot, added);
        if plan.is_ok() != valid {
            return Err("reservation acceptance differs from exact wider sum".into());
        }
        if let Ok(plan) = plan {
            if plan.before() != snapshot
                || plan.reservation_bytes() != added
                || u128::from(plan.reserved_after()) != u128::from(reserved) + u128::from(added)
                || u128::from(plan.total_after()) != total
            {
                return Err("quota plan lost exact accounting".into());
            }
            bump(m, "accepted_quota_reservations");
        } else {
            bump(m, "rejected_quota_reservations");
        }
    } else {
        bump(m, "rejected_quota_snapshots");
    }
    Ok(())
}
fn contents(salt: u8, key: u8) -> Vec<u8> {
    vec![salt.wrapping_add(key); 2 + usize::from(key % 6) * 7 + usize::from(salt % 5)]
}
fn digest(bytes: &[u8]) -> ArtifactDigest {
    ArtifactDigest::new(peritus_codec::sha256(bytes).into_bytes())
}
fn request(bytes: &[u8], style: u8) -> Result<WriteRequest, String> {
    let mut hash = *digest(bytes).as_bytes();
    if style == 2 {
        hash[0] ^= 1;
    }
    Ok(WriteRequest::new(
        ArtifactDigest::new(hash),
        bytes.len() as u64 + u64::from(style == 1),
        if style == 3 { bytes.len() as u64 } else { 64 },
        MediaType::new("application/octet-stream").map_err(|e| e.to_string())?,
        EncryptionMetadata::unencrypted(),
        EventId::new([11; 16]).map_err(|e| format!("{e:?}"))?,
    ))
}
#[derive(Debug)]
enum WriteObservation {
    Rejected(String),
    Abandoned,
    Finalized(peritus_artifact_store::FinalizedArtifact),
}
impl WriteObservation {
    fn require_finalized(self) -> Result<peritus_artifact_store::FinalizedArtifact, String> {
        match self {
            Self::Finalized(receipt) => Ok(receipt),
            other => Err(format!("valid writer did not finalize: {other:?}")),
        }
    }
}
fn observed(
    result: Result<
        peritus_artifact_store::FinalizedArtifact,
        peritus_artifact_store::ArtifactStoreError,
    >,
) -> WriteObservation {
    match result {
        Ok(receipt) => WriteObservation::Finalized(receipt),
        Err(error) => WriteObservation::Rejected(error.to_string()),
    }
}
fn put(
    store: &ArtifactStore,
    bytes: &[u8],
    style: u8,
    owned: bool,
) -> Result<WriteObservation, String> {
    let request = request(bytes, style)?;
    if owned {
        let mut writer = match store.begin_owned_write(request) {
            Ok(writer) => writer,
            Err(error) => return Ok(WriteObservation::Rejected(error.to_string())),
        };
        if style == 3 {
            let oversized = vec![0; bytes.len() + 1];
            if writer.write_chunk(&oversized).is_ok()
                || writer.bytes_written() != 0
                || writer.write_chunk(&[]).is_ok()
            {
                return Err("writer did not atomically reject/poison oversized chunk".into());
            }
            return Ok(observed(store.complete_write(writer)));
        }
        for chunk in bytes.chunks(3) {
            if let Err(error) = writer.write_chunk(chunk) {
                return Ok(WriteObservation::Rejected(error.to_string()));
            }
        }
        if style == 4 {
            return Ok(WriteObservation::Abandoned);
        }
        Ok(observed(store.complete_write(writer)))
    } else {
        let mut writer = match store.begin_write(request) {
            Ok(writer) => writer,
            Err(error) => return Ok(WriteObservation::Rejected(error.to_string())),
        };
        if style == 3 {
            let oversized = vec![0; bytes.len() + 1];
            if writer.write_chunk(&oversized).is_ok()
                || writer.bytes_written() != 0
                || writer.write_chunk(&[]).is_ok()
            {
                return Err(
                    "borrowed writer did not atomically reject/poison oversized chunk".into()
                );
            }
            return Ok(observed(writer.finalize()));
        }
        for chunk in bytes.chunks(5) {
            if let Err(error) = writer.write_chunk(chunk) {
                return Ok(WriteObservation::Rejected(error.to_string()));
            }
        }
        if style == 4 {
            return Ok(WriteObservation::Abandoned);
        }
        Ok(observed(writer.finalize()))
    }
}
#[derive(Clone)]
struct Entry {
    since: Option<u64>,
    owners: BTreeSet<u8>,
}
fn check_store_plan(
    plan: &GcPlan,
    model: &BTreeMap<u8, Entry>,
    salt: u8,
    generation: u64,
) -> Result<(), String> {
    let mut expected = Vec::new();
    let mut marked = BTreeSet::new();
    for (key, row) in model {
        let bytes = contents(salt, *key);
        let digest = digest(&bytes);
        let size = bytes.len() as u64;
        if !row.owners.is_empty() {
            marked.insert(digest);
        }
        if row.since.is_none() && row.owners.is_empty() {
            expected.push(GcAction::Quarantine {
                digest,
                size,
                generation: CollectionGeneration::new(generation).map_err(|e| e.to_string())?,
            });
        } else if let Some(since) = row.since {
            if !row.owners.is_empty() {
                expected.push(GcAction::Restore {
                    digest,
                    size,
                    since: CollectionGeneration::new(since).map_err(|e| e.to_string())?,
                });
            } else if since < generation {
                expected.push(GcAction::Delete {
                    digest,
                    size,
                    since: CollectionGeneration::new(since).map_err(|e| e.to_string())?,
                });
            }
        }
    }
    expected.sort_by_key(|action| action.digest());
    if plan.actions() != expected
        || plan.marked() != &marked
        || plan.generation().get() != generation
    {
        return Err(
            "store collection plan differs from independent live byte/reference ledger".into()
        );
    }
    Ok(())
}
fn owner(id: u8) -> ReferenceOwner {
    if id.is_multiple_of(2) {
        ReferenceOwner::journal(Sha256Digest::new([id; 32]))
    } else {
        ReferenceOwner::evidence(Sha256Digest::new([id; 32]))
    }
}
fn verify(
    store: &ArtifactStore,
    model: &BTreeMap<u8, Entry>,
    salt: u8,
    m: &mut Metrics,
) -> Result<(), String> {
    let roots = store.reference_roots().map_err(|e| e.to_string())?;
    let mut journal = ArtifactReferenceSet::new();
    let mut evidence = ArtifactReferenceSet::new();
    let mut used = 0_u64;
    for key in 0..6 {
        let bytes = contents(salt, key);
        let hash = digest(&bytes);
        let actual = store.metadata(hash).map_err(|e| e.to_string())?;
        if actual.is_some() != model.contains_key(&key) {
            return Err("artifact inventory identity differs".into());
        }
        if let Some(row) = model.get(&key) {
            let actual = actual.ok_or("metadata vanished")?;
            used += bytes.len() as u64;
            let since = match actual.quarantine() {
                QuarantineState::Active => None,
                QuarantineState::Quarantined { since } => Some(since.get()),
            };
            if actual.size() != bytes.len() as u64
                || actual.integrity() != IntegrityState::Healthy
                || since != row.since
            {
                return Err("artifact metadata lost exact state".into());
            }
            if row.since.is_none() {
                if store.read(hash, 64).map_err(|e| e.to_string())? != bytes {
                    return Err("artifact read changed exact bytes".into());
                }
                let mut reader = store.open_read(hash).map_err(|e| e.to_string())?;
                if reader.read_chunk(0).is_ok() || reader.next_offset() != 0 {
                    return Err("invalid read bound advanced reader".into());
                }
                let mut found = Vec::<u8>::new();
                while let Some(chunk) = reader.read_chunk(7).map_err(|e| e.to_string())? {
                    if chunk.offset() != found.len() as u64 {
                        return Err("stream chunk offset skipped/repeated bytes".into());
                    }
                    found.extend(chunk.bytes());
                }
                if found != bytes || reader.remaining_bytes() != 0 {
                    return Err("stream reader did not conserve exact bytes".into());
                }
                bump(m, "verified_stream_reads");
            } else if store.read(hash, 64).is_ok() {
                return Err("quarantined artifact remained readable".into());
            }
            for id in &row.owners {
                if id.is_multiple_of(2) {
                    journal.insert(hash);
                } else {
                    evidence.insert(hash);
                }
            }
        }
    }
    if roots.journal() != &journal
        || roots.evidence() != &evidence
        || store.quota_snapshot(0).map_err(|e| e.to_string())?.used_bytes() != used
    {
        return Err("durable roots/quota differ from independent ledger".into());
    }
    bump(m, "artifact_state_comparisons");
    Ok(())
}
fn history(
    salt: u8,
    limited: bool,
    operations: &[Operation],
    m: &mut Metrics,
) -> Result<(), String> {
    if operations.len() > 128 {
        return Err("artifact history bounds exceeded".into());
    }
    let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
    let config = StoreConfig::new_with_quota_policy(
        temporary.path().join("store"),
        64,
        limited.then_some(96),
    )
    .map_err(|e| e.to_string())?;
    let mut store = ArtifactStore::open(config.clone()).map_err(|e| e.to_string())?;
    let mut model = BTreeMap::<u8, Entry>::new();
    let mut plans: [Option<GcPlan>; 3] = [None, None, None];
    let mut generation = 1_u64;
    for (index, operation) in operations.iter().enumerate() {
        let result = (|| -> Result<ArtifactStore, String> {
            match *operation {
                Operation::Put { key, style, owned } => {
                    let key = key % 6;
                    let bytes = contents(salt, key);
                    let used: usize = model.keys().map(|key| contents(salt, *key).len()).sum();
                    let enough = !limited
                        || used + if model.contains_key(&key) { 0 } else { bytes.len() } <= 96;
                    if style > 4 {
                        return Err("setup-incomplete: unknown writer fixture style".into());
                    }
                    let actual = put(&store, &bytes, style, owned)?;
                    if style == 0 && enough {
                        let receipt = actual.require_finalized()?;
                        let expected = if model.get(&key).is_some_and(|e| e.since.is_none()) {
                            Publication::Existing
                        } else {
                            Publication::New
                        };
                        if receipt.digest() != digest(&bytes)
                            || receipt.size() != bytes.len() as u64
                            || receipt.publication() != expected
                        {
                            return Err("finalization receipt differs".into());
                        }
                        model
                            .entry(key)
                            .or_insert_with(|| Entry { since: None, owners: BTreeSet::new() })
                            .since = None;
                        bump(m, "committed_artifacts");
                    } else if style == 4 && enough {
                        if !matches!(actual, WriteObservation::Abandoned) {
                            return Err("aborted writer published content".into());
                        }
                        bump(m, "abandoned_writers");
                    } else {
                        let WriteObservation::Rejected(reason) = actual else {
                            return Err("invalid/quota-exhausted writer succeeded".into());
                        };
                        if reason.is_empty() {
                            return Err("writer rejection omitted native error".into());
                        }
                        bump(m, "rejected_writers");
                    }
                }
                Operation::Reference { key, owner: id, add } => {
                    let key = key % 6;
                    let hash = digest(&contents(salt, key));
                    if add {
                        let valid = model.get(&key).is_some_and(|e| e.since.is_none());
                        let actual = store.add_reference(owner(id), hash);
                        if actual.is_ok() != valid {
                            return Err("reference admission differs".into());
                        }
                        if valid {
                            model.get_mut(&key).ok_or("entry vanished")?.owners.insert(id);
                            bump(m, "added_references");
                        }
                    } else {
                        let expected = model.get_mut(&key).is_some_and(|e| e.owners.remove(&id));
                        if store.remove_reference(owner(id), hash).map_err(|e| e.to_string())?
                            != expected
                        {
                            return Err("reference removal result differs".into());
                        }
                        bump(m, "reference_removal_attempts");
                    }
                }
                Operation::Plan { slot, advance } => {
                    if advance {
                        generation += 1;
                    }
                    plans[usize::from(slot) % 3] = Some(
                        store
                            .plan_gc(
                                CollectionGeneration::new(generation).map_err(|e| e.to_string())?,
                            )
                            .map_err(|e| e.to_string())?,
                    );
                    check_store_plan(
                        plans[usize::from(slot) % 3].as_ref().ok_or("plan vanished")?,
                        &model,
                        salt,
                        generation,
                    )?;
                    bump(m, "verified_store_plan_bindings");
                    bump(m, "saved_gc_plans");
                }
                Operation::Apply(slot) => {
                    if let Some(plan) = &plans[usize::from(slot) % 3] {
                        let mut expected_ok = true;
                        let mut counts = [0_u64; 4];
                        for action in plan.actions() {
                            let Some(key) =
                                (0..6).find(|key| digest(&contents(salt, *key)) == action.digest())
                            else {
                                return Err("plan invented unknown artifact".into());
                            };
                            let Some(row) = model.get_mut(&key) else {
                                expected_ok = false;
                                break;
                            };
                            let valid = match *action {
                                GcAction::Quarantine { .. } => {
                                    row.since.is_none() && row.owners.is_empty()
                                }
                                GcAction::Restore { since, .. } => row.since == Some(since.get()),
                                GcAction::Delete { since, .. } => {
                                    row.since == Some(since.get()) && row.owners.is_empty()
                                }
                            };
                            if !valid {
                                expected_ok = false;
                                break;
                            }
                            match *action {
                                GcAction::Quarantine { generation, .. } => {
                                    row.since = Some(generation.get());
                                    counts[0] += 1;
                                }
                                GcAction::Restore { .. } => {
                                    row.since = None;
                                    counts[1] += 1;
                                }
                                GcAction::Delete { size, .. } => {
                                    model.remove(&key);
                                    counts[2] += 1;
                                    counts[3] += size;
                                }
                            }
                        }
                        let actual = store.apply_gc_plan(plan);
                        if actual.is_ok() != expected_ok {
                            return Err(format!(
                                "stale/current GC plan application differs: {actual:?}"
                            ));
                        }
                        if let Ok(actual) = actual {
                            if [
                                actual.quarantined(),
                                actual.restored(),
                                actual.deleted(),
                                actual.deleted_bytes(),
                            ] != counts
                            {
                                return Err("GC successful prefix accounting differs".into());
                            }
                            bump(m, "applied_gc_plans");
                        } else {
                            bump(m, "rejected_stale_gc_plans");
                        }
                    }
                }
                Operation::Reopen => {
                    drop(store);
                    store = ArtifactStore::open(config.clone()).map_err(|e| e.to_string())?;
                    bump(m, "artifact_reopens");
                }
                Operation::Read(key) => {
                    let key = key % 6;
                    let actual = store.read(digest(&contents(salt, key)), 64);
                    let valid = model.get(&key).is_some_and(|e| e.since.is_none());
                    if actual.is_ok() != valid {
                        return Err("explicit read acceptance differs".into());
                    }
                    bump(m, "explicit_artifact_reads");
                }
            }
            verify(&store, &model, salt, m)?;
            Ok(store)
        })();
        store = result.map_err(|e| format!("{e}; operation {index}: {operation:?}"))?;
        bump(m, "history_operations");
    }
    Ok(())
}
fn object_path(root: &Path, hash: ArtifactDigest, quarantined: bool) -> std::path::PathBuf {
    let name = hash.to_hex();
    root.join(if quarantined { "quarantine/sha256" } else { "objects/sha256" })
        .join(&name[..2])
        .join(name)
}
fn recovery(cut: u8, payload: &[u8], m: &mut Metrics) -> Result<(), String> {
    if payload.is_empty() || payload.len() > 128 {
        return Err("recovery payload bounds exceeded".into());
    }
    let temporary = tempfile::tempdir().map_err(|e| e.to_string())?;
    let root = temporary.path().join("store");
    let config = StoreConfig::new(root.clone(), 128, 512).map_err(|e| e.to_string())?;
    let mut store = ArtifactStore::open(config.clone()).map_err(|e| e.to_string())?;
    let hash = digest(payload);
    match cut % 4 {
        0 => {
            let db = rusqlite::Connection::open(root.join("metadata.sqlite3"))
                .map_err(|e| e.to_string())?;
            db.execute_batch("CREATE TRIGGER praxis_fail BEFORE INSERT ON artifact_records BEGIN SELECT RAISE(ABORT,'injected catalog publication failure'); END;").map_err(|e|e.to_string())?;
            if !matches!(put(&store, payload, 0, false)?, WriteObservation::Rejected(_)) {
                return Err("injected catalog failure was not reached".into());
            }
            if fs::read(object_path(&root, hash, false)).map_err(|e| e.to_string())? != payload
                || store.metadata(hash).map_err(|e| e.to_string())?.is_some()
            {
                return Err("publication-before-catalog window not actually reached".into());
            }
            db.execute_batch("DROP TRIGGER praxis_fail").map_err(|e| e.to_string())?;
            drop(db);
            drop(store);
            store = ArtifactStore::open(config.clone()).map_err(|e| e.to_string())?;
            if store.read(hash, 128).is_ok()
                || fs::read(object_path(&root, hash, true)).map_err(|e| e.to_string())? != payload
            {
                return Err("unrecorded publication was not conservatively quarantined".into());
            }
            drop(store);
            store = ArtifactStore::open(config.clone()).map_err(|e| e.to_string())?;
            if object_path(&root, hash, true).exists() {
                return Err("later orphan recovery did not finish removal".into());
            }
            bump(m, "publication_catalog_failure_recovery");
        }
        1 => {
            put(&store, payload, 0, true)?.require_finalized()?;
            let plan = store
                .plan_gc(CollectionGeneration::new(1).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
            let name = hash.to_hex();
            let blocker = root.join("quarantine/sha256").join(&name[..2]);
            fs::write(&blocker, b"block directory publication").map_err(|e| e.to_string())?;
            if store.apply_gc_plan(&plan).is_ok() {
                return Err("quarantine movement failure was not reached".into());
            }
            if !matches!(
                store
                    .metadata(hash)
                    .map_err(|e| e.to_string())?
                    .ok_or("metadata vanished")?
                    .quarantine(),
                QuarantineState::Quarantined { .. }
            ) || !object_path(&root, hash, false).exists()
            {
                return Err("metadata-first quarantine window not reached".into());
            }
            fs::remove_file(blocker).map_err(|e| e.to_string())?;
            drop(store);
            store = ArtifactStore::open(config.clone()).map_err(|e| e.to_string())?;
            if object_path(&root, hash, false).exists()
                || fs::read(object_path(&root, hash, true)).map_err(|e| e.to_string())? != payload
            {
                return Err("reopen failed to complete quarantine move".into());
            }
            bump(m, "quarantine_move_failure_recovery");
        }
        2 => {
            put(&store, payload, 0, false)?.require_finalized()?;
            fs::write(object_path(&root, hash, false), vec![payload[0] ^ 1; payload.len()])
                .map_err(|e| e.to_string())?;
            if store.read(hash, 128).is_ok() {
                return Err("divergent active bytes passed exact read".into());
            }
            drop(store);
            store = ArtifactStore::open(config.clone()).map_err(|e| e.to_string())?;
            let metadata = store
                .metadata(hash)
                .map_err(|e| e.to_string())?
                .ok_or("corruption lost metadata")?;
            if metadata.integrity() != IntegrityState::Corrupt
                || metadata.is_referenceable()
                || object_path(&root, hash, false).exists()
            {
                return Err("recovery failed to durably contain corruption".into());
            }
            drop(store);
            store = ArtifactStore::open(config.clone()).map_err(|e| e.to_string())?;
            if store.read(hash, 128).is_ok() {
                return Err("corruption became readable after second reopen".into());
            }
            bump(m, "corruption_containment_reopens");
        }
        _ => {
            put(&store, payload, 0, true)?.require_finalized()?;
            let original = object_path(&root, hash, false);
            let quarantine = object_path(&root, hash, true);
            fs::create_dir_all(quarantine.parent().ok_or("no quarantine parent")?)
                .map_err(|e| e.to_string())?;
            fs::rename(&original, &quarantine).map_err(|e| e.to_string())?;
            drop(store);
            store = ArtifactStore::open(config.clone()).map_err(|e| e.to_string())?;
            if store.read(hash, 128).map_err(|e| e.to_string())? != payload || quarantine.exists() {
                return Err("active catalog recovery did not restore displaced bytes".into());
            }
            bump(m, "constructed_restore_image_recovery");
        }
    }
    let empty = store.recover().map_err(|e| e.to_string())?;
    if empty.removed_temporary_files() != 0
        || empty.completed_state_moves() != 0
        || empty.removed_swept_files() != 0
        || !empty.contained_corruptions().is_empty()
        || !empty.quarantined_orphans().is_empty()
    {
        return Err("repeated recovery was not idempotent".into());
    }
    Ok(())
}
pub fn check(case: &Case) -> Result<Metrics, String> {
    let mut m = Metrics::new();
    bump(&mut m, "cases");
    match case {
        Case::Gc { .. } => gc(case, &mut m)?,
        Case::Quota { used, reserved, limit, added } => {
            quota(*used, *reserved, *limit, *added, &mut m)?
        }
        Case::History { salt, limited, operations } => {
            history(*salt, *limited, operations, &mut m)?
        }
        Case::Recovery { cut, payload } => recovery(*cut, payload, &mut m)?,
    }
    Ok(m)
}
pub fn calibration_case() -> Case {
    Case::Gc {
        generation: 2,
        rows: vec![Row { key: 1, size: 7, since: Some(2) }],
        journal: vec![],
        evidence: vec![],
    }
}
pub fn examples() -> Vec<Case> {
    let mut cases = vec![
        calibration_case(),
        Case::Gc {
            generation: 1,
            rows: vec![Row { key: 1, size: 7, since: Some(2) }],
            journal: vec![1],
            evidence: vec![],
        },
        Case::Gc {
            generation: 3,
            rows: vec![Row { key: 1, size: 7, since: Some(2) }],
            journal: vec![1],
            evidence: vec![],
        },
    ];
    for cut in 0..4 {
        cases.push(Case::Recovery { cut, payload: b"exact durable bytes".to_vec() });
    }
    for limit in [None, Some(0), Some(u64::MAX)] {
        cases.push(Case::Quota { used: u64::MAX, reserved: 0, limit, added: 1 });
    }
    cases.push(Case::History {
        salt: 0,
        limited: true,
        operations: vec![
            Operation::Put { key: 0, style: 0, owned: false },
            Operation::Reference { key: 0, owner: 0, add: true },
            Operation::Plan { slot: 0, advance: true },
            Operation::Apply(0),
            Operation::Reference { key: 0, owner: 0, add: false },
            Operation::Plan { slot: 0, advance: true },
            Operation::Apply(0),
            Operation::Reopen,
            Operation::Put { key: 0, style: 0, owned: true },
            Operation::Reopen,
        ],
    });
    cases
}

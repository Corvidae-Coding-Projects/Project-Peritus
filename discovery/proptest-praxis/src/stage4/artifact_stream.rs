//! Public owned-stream histories with an independent original/object/observed byte oracle.

use std::{
    fs::{self, OpenOptions},
    io::{Seek, SeekFrom, Write},
    path::Path,
};

use peritus_artifact_store::{
    ArtifactDigest, ArtifactReadHandle, ArtifactStore, EncryptionMetadata, ErrorCode, MediaType,
    StoreConfig, WriteRequest,
};
use peritus_types::EventId;
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const LIMIT: u16 = 256;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub original: Vec<u8>,
    pub pre_open_corruption: bool,
    pub operations: Vec<Operation>,
    pub finish_bound: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    Read { bound: u16 },
    Xor { position: u16, mask: u8 },
    Resize { size: u16 },
    Append { byte: u8 },
    Restore,
}

fn bounds() -> BoxedStrategy<u16> {
    prop_oneof![4 => 1_u16..=LIMIT, 2 => Just(0), 2 => Just(LIMIT + 1), 1 => Just(1), 1 => Just(LIMIT)].boxed()
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let steps = max_steps.min(64);
    let operation = prop_oneof![
        7 => bounds().prop_map(|bound| Operation::Read { bound }),
        4 => (any::<u16>(), 1_u8..=u8::MAX).prop_map(|(position,mask)| Operation::Xor {position,mask}),
        2 => (0_u16..=LIMIT).prop_map(|size| Operation::Resize {size}),
        1 => any::<u8>().prop_map(|byte| Operation::Append {byte}),
        2 => Just(Operation::Restore),
    ];
    let general = (
        prop_oneof![5 => prop::collection::vec(any::<u8>(), 0..=128), 1 => prop::collection::vec(any::<u8>(), 256)],
        prop::collection::vec(operation, 0..=steps),
        1_u16..=LIMIT,
    ).prop_map(|(original,operations,finish_bound)| Case { original, operations, finish_bound, pre_open_corruption:false });
    let correlated = (
        prop::collection::vec(any::<u8>(), 2..=128),
        any::<u16>(),
        any::<u16>(),
        1_u8..=u8::MAX,
        any::<bool>(),
        bounds(),
    )
        .prop_map(move |(original, prefix, position, mask, consumed, finish_bound)| {
            let prefix = 1 + usize::from(prefix) % (original.len() - 1);
            let position = if consumed {
                usize::from(position) % prefix
            } else {
                prefix + usize::from(position) % (original.len() - prefix)
            };
            let mut operations = vec![
                Operation::Read { bound: 0 },
                Operation::Read { bound: prefix as u16 },
                Operation::Xor { position: position as u16, mask },
                Operation::Read { bound: LIMIT + 1 },
            ];
            operations.truncate(steps);
            Case {
                original,
                operations,
                pre_open_corruption: false,
                finish_bound: finish_bound.clamp(1, LIMIT),
            }
        });
    let before_first =
        (prop::collection::vec(any::<u8>(), 1..=128), any::<u16>(), 1_u8..=u8::MAX, 1_u16..=LIMIT)
            .prop_map(move |(original, position, mask, finish_bound)| Case {
                original,
                pre_open_corruption: false,
                operations: if steps == 0 {
                    vec![]
                } else {
                    vec![Operation::Xor { position, mask }]
                },
                finish_bound,
            });
    let pre_open = prop::collection::vec(any::<u8>(), 0..=128).prop_map(|original| Case {
        original,
        pre_open_corruption: true,
        operations: vec![],
        finish_bound: 1,
    });
    prop_oneof![5=>general,4=>correlated,3=>before_first,1=>pre_open].boxed()
}

fn bump(metrics: &mut crate::Metrics, name: &str) {
    *metrics.entry(name.into()).or_default() += 1;
}

fn hash(bytes: &[u8]) -> ArtifactDigest {
    ArtifactDigest::new(Sha256::digest(bytes).into())
}

struct Oracle {
    original: Vec<u8>,
    object: Vec<u8>,
    observed: Vec<u8>,
    failed: bool,
    completed: bool,
}

impl Oracle {
    fn fault_visible(&self) -> bool {
        self.object != self.original
            || self.original.get(..self.observed.len()) != Some(self.observed.as_slice())
    }

    fn read(
        &mut self,
        reader: &mut ArtifactReadHandle,
        bound: u16,
        metrics: &mut crate::Metrics,
    ) -> Result<(), String> {
        let offset = reader.next_offset();
        let remaining = reader.remaining_bytes();
        let result = reader.read_chunk(usize::from(bound));
        bump(metrics, "read_calls");
        if bound == 0 || bound > LIMIT {
            match result {
                Err(error) if error.code() == ErrorCode::ByteLimitExceeded => {}
                other => return Err(format!("invalid bound was not rejected: {other:?}")),
            }
            if reader.next_offset() != offset || reader.remaining_bytes() != remaining {
                return Err("invalid bound changed the exact stream cursor".into());
            }
            bump(metrics, "reversible_bound_rejections");
            return Ok(());
        }
        if self.failed {
            if result.is_ok()
                || reader.next_offset() != offset
                || reader.remaining_bytes() != remaining
            {
                return Err("failed stream resumed or changed cursor on retry".into());
            }
            bump(metrics, "retained_terminal_failures");
            return Ok(());
        }
        if self.completed {
            if !matches!(result, Ok(None)) || reader.next_offset() != offset {
                return Err("completed stream did not retain exact EOF".into());
            }
            bump(metrics, "repeated_eof");
            return Ok(());
        }
        match result {
            Ok(Some(chunk)) => {
                let start = self.observed.len();
                let end = start + chunk.bytes().len();
                if chunk.offset() != start as u64
                    || chunk.bytes().is_empty()
                    || chunk.bytes().len() > usize::from(bound)
                    || end > self.original.len()
                    || self.object.get(start..end) != Some(chunk.bytes())
                    || reader.next_offset() != end as u64
                {
                    return Err(
                        "chunk bytes, exact offset or bound differ from the independent object"
                            .into(),
                    );
                }
                self.observed.extend_from_slice(chunk.bytes());
                bump(metrics, "returned_chunks");
            }
            Ok(None) => {
                if self.observed != self.original || hash(&self.observed) != hash(&self.original) {
                    return Err(format!(
                        "stream completed with bytes differing from its original verified content: expected {:?}, observed {:?}",
                        self.original, self.observed
                    ));
                }
                if reader.remaining_bytes() != 0
                    || reader.next_offset() != self.original.len() as u64
                {
                    return Err("successful completion lost exact cursor/size".into());
                }
                self.completed = true;
                bump(metrics, "authenticated_completions");
                if self.object != self.original {
                    bump(metrics, "completed_original_bytes_with_changed_object");
                }
            }
            Err(error) => {
                if !self.fault_visible() {
                    return Err(format!("uncorrupted stream rejected a valid read: {error}"));
                }
                self.failed = true;
                bump(metrics, "detected_stream_faults");
                if self.object.len() == self.original.len() {
                    bump(metrics, "detected_same_size_faults");
                }
            }
        }
        if reader.next_offset() > self.original.len() as u64
            || reader.remaining_bytes() != self.original.len() as u64 - reader.next_offset()
        {
            return Err("reader cursor escaped the declared artifact size".into());
        }
        Ok(())
    }

    fn mutate(
        &mut self,
        operation: &Operation,
        path: &Path,
        metrics: &mut crate::Metrics,
    ) -> Result<(), String> {
        match operation {
            Operation::Read { .. } => return Err("read operation entered mutation adapter".into()),
            Operation::Xor { position, mask } => {
                if *mask == 0 {
                    return Err("replay XOR mask must be nonzero".into());
                }
                if self.object.is_empty() {
                    self.object.push(*mask);
                    bump(metrics, "xor_empty_growths");
                } else {
                    let position = usize::from(*position) % self.object.len();
                    self.object[position] ^= mask;
                    if position < self.observed.len() {
                        bump(metrics, "consumed_prefix_overwrites");
                    } else {
                        bump(metrics, "unread_overwrites");
                    }
                }
            }
            Operation::Resize { size } => {
                if self.object.len() != usize::from(*size) {
                    bump(metrics, "length_mutations");
                }
                self.object.resize(usize::from(*size), 0);
                bump(metrics, "resize_calls");
            }
            Operation::Append { byte } => {
                self.object.push(*byte);
                bump(metrics, "appends");
            }
            Operation::Restore => {
                self.object.clone_from(&self.original);
                bump(metrics, "external_restores");
            }
        }
        // Write in place so the already-open file handle observes the controlled change.
        let mut file = OpenOptions::new().write(true).open(path).map_err(|e| e.to_string())?;
        file.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
        file.write_all(&self.object).map_err(|e| e.to_string())?;
        file.set_len(self.object.len() as u64).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        if fs::read(path).map_err(|e| e.to_string())? != self.object {
            return Err("controlled external write did not reach the real object".into());
        }
        Ok(())
    }
}

pub fn check(case: &Case) -> Result<crate::Metrics, String> {
    if case.original.len() > usize::from(LIMIT)
        || case.operations.len() > 64
        || case.finish_bound == 0
        || case.finish_bound > LIMIT
        || case
            .operations
            .iter()
            .any(|operation| matches!(operation,Operation::Resize {size} if *size > LIMIT))
    {
        return Err("artifact-stream case exceeds its explicit fixture bounds".into());
    }
    let mut metrics = crate::Metrics::new();
    let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
    let root = directory.path().join("artifacts");
    let config = StoreConfig::new(&root, u64::from(LIMIT), 4096).map_err(|e| e.to_string())?;
    let store = ArtifactStore::open(config).map_err(|e| e.to_string())?;
    let digest = hash(&case.original);
    let event = EventId::new([31; 16]).map_err(|e| format!("{e:?}"))?;
    let mut writer = store
        .begin_owned_write(WriteRequest::new(
            digest,
            case.original.len() as u64,
            u64::from(LIMIT),
            MediaType::new("application/octet-stream").map_err(|e| e.to_string())?,
            EncryptionMetadata::unencrypted(),
            event,
        ))
        .map_err(|e| e.to_string())?;
    writer.write_chunk(&case.original).map_err(|e| e.to_string())?;
    store.complete_write(writer).map_err(|e| e.to_string())?;
    if store.read(digest, u64::from(LIMIT)).map_err(|e| e.to_string())? != case.original {
        return Err("initial finalized bytes differ from the independent input".into());
    }
    let hex = digest.to_hex();
    let path = root.join("objects/sha256").join(&hex[..2]).join(&hex);
    let mut oracle = Oracle {
        original: case.original.clone(),
        object: case.original.clone(),
        observed: vec![],
        failed: false,
        completed: false,
    };
    if case.pre_open_corruption {
        oracle.mutate(&Operation::Xor { position: 0, mask: 1 }, &path, &mut metrics)?;
        match store.open_read(digest) {
            Err(error) if error.code() == ErrorCode::CorruptObject => {}
            _ => return Err("pre-open corruption did not reject before reader creation".into()),
        }
        bump(&mut metrics, "pre_open_corruption_rejections");
        return Ok(metrics);
    }
    let mut reader = store.open_read(digest).map_err(|e| e.to_string())?;
    bump(&mut metrics, "verified_reader_opens");
    for operation in &case.operations {
        bump(&mut metrics, "operations");
        match operation {
            Operation::Read { bound } => oracle.read(&mut reader, *bound, &mut metrics)?,
            mutation => oracle.mutate(mutation, &path, &mut metrics)?,
        }
        if reader.metadata().digest() != digest
            || reader.metadata().size() != case.original.len() as u64
        {
            return Err("stream metadata changed after opening".into());
        }
    }
    for _ in 0..=case.original.len() + 1 {
        if oracle.completed || oracle.failed {
            break;
        }
        oracle.read(&mut reader, case.finish_bound, &mut metrics)?;
    }
    if !oracle.completed && !oracle.failed {
        return Err("bounded drain failed to reach a terminal stream observation".into());
    }
    oracle.read(&mut reader, 0, &mut metrics)?;
    oracle.read(&mut reader, LIMIT + 1, &mut metrics)?;
    oracle.mutate(&Operation::Restore, &path, &mut metrics)?;
    for bound in [1, 7, LIMIT] {
        oracle.read(&mut reader, bound, &mut metrics)?;
    }
    Ok(metrics)
}

pub fn calibration() -> Case {
    Case {
        original: vec![10, 20, 30, 40],
        pre_open_corruption: false,
        operations: vec![Operation::Xor { position: 1, mask: 1 }],
        finish_bound: 2,
    }
}

pub fn examples() -> Vec<Case> {
    let plain = |original: Vec<u8>, operations: Vec<Operation>| Case {
        original,
        operations,
        pre_open_corruption: false,
        finish_bound: 3,
    };
    vec![
        plain(vec![], vec![]),
        plain(vec![], vec![Operation::Append { byte: 9 }]),
        plain(
            (0..16).collect(),
            vec![
                Operation::Read { bound: 0 },
                Operation::Read { bound: 257 },
                Operation::Read { bound: 1 },
            ],
        ),
        calibration(),
        plain(
            (0..16).collect(),
            vec![Operation::Read { bound: 4 }, Operation::Xor { position: 8, mask: 255 }],
        ),
        plain(
            (0..16).collect(),
            vec![Operation::Read { bound: 4 }, Operation::Xor { position: 0, mask: 1 }],
        ),
        plain((0..16).collect(), vec![Operation::Resize { size: 15 }]),
        plain((0..16).collect(), vec![Operation::Resize { size: 17 }]),
        plain((0..16).collect(), vec![Operation::Read { bound: 3 }, Operation::Resize { size: 4 }]),
        plain(
            (0..16).collect(),
            vec![
                Operation::Append { byte: 4 },
                Operation::Read { bound: 256 },
                Operation::Restore,
                Operation::Read { bound: 256 },
            ],
        ),
        plain(
            (0..16).collect(),
            vec![
                Operation::Read { bound: 256 },
                Operation::Read { bound: 1 },
                Operation::Append { byte: 3 },
                Operation::Read { bound: 3 },
            ],
        ),
        plain(vec![255; 256], vec![Operation::Read { bound: 256 }]),
        Case { pre_open_corruption: true, ..calibration() },
    ]
}

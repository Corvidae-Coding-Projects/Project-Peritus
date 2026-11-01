//! Exact C1 file inspection against an independent byte/line cursor model.
//!
//! This is the opened-folder reader consumed by the filesystem tool. It exercises a real
//! disposable file, full-source SHA-256, selected pages, rewrite history, cancellation and
//! no-follow behavior without granting a workspace mutation permit.

use crate::Metrics;
use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;
use peritus_workspace::{FileReadSelection, FolderIdentity, FolderInspection};
use proptest::prelude::*;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};
use std::fs;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Case {
    pub initial: Vec<u8>,
    pub operations: Vec<Operation>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Operation {
    Bytes { start: u16, end: u16, maximum: u16 },
    Lines { first: u8, last: u8, maximum: u16 },
    All { maximum: u16 },
    Rewrite(Vec<u8>),
    Scan { cancel_before: bool },
    Symlink,
    CancelAfterFirstChunk,
    MutateDuringScan { replace_path: bool },
}

fn bump(metrics: &mut Metrics, name: &str) {
    *metrics.entry(name.into()).or_default() += 1;
}

fn bytes() -> BoxedStrategy<Vec<u8>> {
    prop_oneof![
        4 => prop::collection::vec(any::<u8>(), 0..128),
        2 => prop::sample::select(vec![b"\r\n\n".to_vec(), "é👩‍💻\nnext".as_bytes().to_vec(), vec![0, 0xff, b'\n']]),
    ].boxed()
}

fn operation() -> BoxedStrategy<Operation> {
    prop_oneof![
        5 => (0_u16..160, 0_u16..160, 0_u16..160)
            .prop_map(|(start,end,maximum)| Operation::Bytes{start,end,maximum}),
        5 => (0_u8..12, 0_u8..12, 0_u16..160)
            .prop_map(|(first,last,maximum)| Operation::Lines{first,last,maximum}),
        3 => (0_u16..160).prop_map(|maximum| Operation::All{maximum}),
        3 => bytes().prop_map(Operation::Rewrite),
        2 => any::<bool>().prop_map(|cancel_before| Operation::Scan{cancel_before}),
        1 => Just(Operation::Symlink),
    ]
    .boxed()
}

pub fn strategy(max_steps: usize) -> BoxedStrategy<Case> {
    let mixed = (bytes(), prop::collection::vec(operation(), 1..=max_steps.clamp(1, 12)))
        .prop_map(|(initial, operations)| Case { initial, operations })
        .boxed();
    let eof = bytes().prop_map(|initial| Case {
        operations: vec![Operation::Bytes {
            start: 0,
            end: u16::try_from(initial.len()).unwrap_or(u16::MAX),
            maximum: u16::try_from(initial.len().max(1)).unwrap_or(u16::MAX),
        }],
        initial,
    });
    let continued_line =
        (prop::collection::vec(0_u8..26, 2..48), any::<u8>()).prop_map(|(letters, selector)| {
            let maximum = 1 + usize::from(selector) % letters.len();
            let mut initial = letters.into_iter().map(|value| b'a' + value).collect::<Vec<_>>();
            initial.push(b'\n');
            Case {
                initial,
                operations: vec![Operation::Lines {
                    first: 1,
                    last: 1,
                    maximum: u16::try_from(maximum).unwrap_or(u16::MAX),
                }],
            }
        });
    let interrupted = (
        prop::sample::select(vec![1_usize, 65_535, 65_536, 65_537, 131_072]),
        any::<u8>(),
        any::<bool>(),
    )
        .prop_map(|(length, byte, replace_path)| Case {
            initial: vec![byte; length],
            operations: vec![
                Operation::CancelAfterFirstChunk,
                Operation::MutateDuringScan { replace_path },
                Operation::Scan { cancel_before: false },
            ],
        });
    prop_oneof![4 => mixed, 2 => eof, 2 => continued_line, 2 => interrupted].boxed()
}

fn sha256(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::new(Sha256::digest(bytes).into())
}

#[derive(Debug, Eq, PartialEq)]
struct Expected {
    range: (u64, u64),
    bytes: Vec<u8>,
    continuation: Option<u64>,
}

fn expected_bytes(source: &[u8], start: u64, end: u64, maximum: u64) -> Option<Expected> {
    if maximum == 0 || start > end || end > source.len() as u64 || end - start > maximum {
        return None;
    }
    Some(Expected {
        range: (start, end),
        bytes: source[usize::try_from(start).ok()?..usize::try_from(end).ok()?].to_vec(),
        continuation: None,
    })
}

fn expected_lines(source: &[u8], first: u64, last: u64, maximum: u64) -> Option<Expected> {
    if maximum == 0 || first == 0 || first > last {
        return None;
    }
    let mut line = 1_u64;
    let mut last_seen = 0_u64;
    let mut chosen = Vec::new();
    for (offset, byte) in source.iter().enumerate() {
        last_seen = line;
        if first <= line && line <= last {
            chosen.push((offset as u64, *byte));
        }
        if *byte == b'\n' {
            line += 1;
        }
    }
    if last_seen < last {
        return None;
    }
    let retained = chosen.iter().take(maximum as usize).copied().collect::<Vec<_>>();
    Some(Expected {
        range: retained
            .first()
            .zip(retained.last())
            .map_or((0, 0), |(start, end)| (start.0, end.0 + 1)),
        bytes: retained.iter().map(|(_, byte)| *byte).collect(),
        continuation: chosen.get(maximum as usize).map(|(offset, _)| *offset),
    })
}

fn expected_all(source: &[u8], maximum: u64) -> Option<Expected> {
    if maximum == 0 || source.len() as u64 > maximum {
        return None;
    }
    Some(Expected { range: (0, source.len() as u64), bytes: source.to_vec(), continuation: None })
}

fn check_selection(
    inspection: &FolderInspection,
    path: &WorkspacePath,
    source: &[u8],
    selection: Result<FileReadSelection, peritus_workspace::WorkspaceError>,
    maximum: u64,
    expected: Option<Expected>,
    metrics: &mut Metrics,
) -> Result<(), String> {
    let actual = selection.and_then(|selection| inspection.read_file(path, selection, maximum));
    let Some(expected) = expected else {
        if actual.is_ok() {
            return Err(format!("invalid file selection was accepted: {actual:?}"));
        }
        bump(metrics, "selection_rejections");
        return Ok(());
    };
    let observed = actual.map_err(|error| format!("valid file selection rejected: {error}"))?;
    if observed.bytes() != expected.bytes.as_slice()
        || observed.range() != expected.range
        || observed.continuation_offset() != expected.continuation
        || observed.source_bytes() != source.len() as u64
        || observed.source_digest() != sha256(source)
        || observed.digest() != sha256(&expected.bytes)
    {
        return Err(format!(
            "file selection differs: expected {expected:?}, observed {observed:?}"
        ));
    }
    bump(metrics, "selected_pages");
    if expected.continuation.is_some() {
        bump(metrics, "continued_lines");
    }
    Ok(())
}

pub fn check(case: &Case) -> Result<Metrics, String> {
    if case.initial.len() > 131_072
        || case.operations.len() > 16
        || case.operations.iter().any(
            |operation| matches!(operation, Operation::Rewrite(bytes) if bytes.len() > 131_072),
        )
    {
        return Err("setup-incomplete: scoped read case bounds".into());
    }
    let temporary = tempfile::tempdir().map_err(|error| error.to_string())?;
    let root = temporary.path().join("selected");
    fs::create_dir(&root).map_err(|error| error.to_string())?;
    let outside = temporary.path().join("outside-canary");
    fs::write(&outside, b"unrelated-host-content").map_err(|error| error.to_string())?;
    let file = root.join("source.bin");
    fs::write(&file, &case.initial).map_err(|error| error.to_string())?;
    let identity = FolderIdentity::observe(&root).map_err(|error| error.to_string())?;
    let inspection = FolderInspection::open(&identity).map_err(|error| error.to_string())?;
    let path = WorkspacePath::new("source.bin").map_err(|error| error.to_string())?;
    let mut source = case.initial.clone();
    let mut metrics = Metrics::new();
    bump(&mut metrics, "cases");
    for operation in &case.operations {
        match operation {
            Operation::Bytes { start, end, maximum } => {
                check_selection(
                    &inspection,
                    &path,
                    &source,
                    FileReadSelection::bytes(u64::from(*start), u64::from(*end)),
                    u64::from(*maximum),
                    expected_bytes(
                        &source,
                        u64::from(*start),
                        u64::from(*end),
                        u64::from(*maximum),
                    ),
                    &mut metrics,
                )?;
            }
            Operation::Lines { first, last, maximum } => {
                check_selection(
                    &inspection,
                    &path,
                    &source,
                    FileReadSelection::lines(u64::from(*first), u64::from(*last)),
                    u64::from(*maximum),
                    expected_lines(
                        &source,
                        u64::from(*first),
                        u64::from(*last),
                        u64::from(*maximum),
                    ),
                    &mut metrics,
                )?;
            }
            Operation::All { maximum } => {
                check_selection(
                    &inspection,
                    &path,
                    &source,
                    Ok(FileReadSelection::all()),
                    u64::from(*maximum),
                    expected_all(&source, u64::from(*maximum)),
                    &mut metrics,
                )?;
            }
            Operation::Rewrite(bytes) => {
                fs::write(&file, bytes).map_err(|error| error.to_string())?;
                source = bytes.clone();
                bump(&mut metrics, "rewrites");
            }
            Operation::Scan { cancel_before } => {
                let mut visited: Vec<u8> = Vec::new();
                let observed = inspection
                    .scan_file_chunks(
                        &path,
                        || *cancel_before,
                        |offset, bytes| {
                            if offset != visited.len() as u64 {
                                return;
                            }
                            visited.extend(bytes);
                        },
                    )
                    .map_err(|error| error.to_string())?;
                if *cancel_before {
                    if observed.is_some() || !visited.is_empty() {
                        return Err("pre-scan cancellation read or published bytes".into());
                    }
                    bump(&mut metrics, "cancelled_scans");
                } else if observed != Some((source.len() as u64, sha256(&source)))
                    || visited != source
                {
                    return Err("streamed source bytes/offset/digest differ".into());
                } else {
                    bump(&mut metrics, "completed_scans");
                }
            }
            Operation::CancelAfterFirstChunk => {
                let visited = std::cell::Cell::new(0_usize);
                let mut bytes = Vec::new();
                let observed = inspection
                    .scan_file_chunks(
                        &path,
                        || visited.get() > 0,
                        |offset, chunk| {
                            assert_eq!(offset, bytes.len() as u64);
                            bytes.extend_from_slice(chunk);
                            visited.set(visited.get() + 1);
                        },
                    )
                    .map_err(|error| error.to_string())?;
                if source.is_empty() {
                    if observed != Some((0, sha256(&[]))) || !bytes.is_empty() {
                        return Err("empty cancelled scan published inconsistent content".into());
                    }
                } else if observed.is_some()
                    || visited.get() != 1
                    || bytes.is_empty()
                    || !source.starts_with(&bytes)
                {
                    return Err(
                        "mid-scan cancellation published a final digest or kept visiting".into()
                    );
                } else {
                    bump(&mut metrics, "mid_scan_cancellations");
                }
            }
            Operation::MutateDuringScan { replace_path } => {
                if source.is_empty() {
                    continue;
                }
                let mut replacement = source.clone();
                replacement[0] ^= 0xff; // same-size change, never an equal rewrite
                let mut changed = false;
                let mut mutation_error = None;
                let observed = inspection.scan_file_chunks(
                    &path,
                    || false,
                    |_, _| {
                        if changed {
                            return;
                        }
                        changed = true;
                        let written = if *replace_path {
                            let alternate = root.join("replacement.bin");
                            fs::write(&alternate, &replacement)
                                .and_then(|()| fs::rename(&alternate, &file))
                        } else {
                            fs::write(&file, &replacement)
                        };
                        mutation_error = written.err();
                    },
                );
                if let Some(error) = mutation_error {
                    return Err(error.to_string());
                }
                if !changed || observed.is_ok() {
                    return Err(
                        "scan published a digest after its source changed during visitation".into(),
                    );
                }
                source = replacement;
                bump(
                    &mut metrics,
                    if *replace_path {
                        "scan_path_replacement_rejections"
                    } else {
                        "scan_in_place_mutation_rejections"
                    },
                );
            }
            Operation::Symlink => {
                #[cfg(unix)]
                {
                    use std::os::unix::fs::symlink;
                    fs::remove_file(&file).map_err(|error| error.to_string())?;
                    symlink(&outside, &file).map_err(|error| error.to_string())?;
                    if inspection.read_file(&path, FileReadSelection::all(), 131_072).is_ok() {
                        return Err("symlink source was followed".into());
                    }
                    fs::remove_file(&file).map_err(|error| error.to_string())?;
                    fs::write(&file, &source).map_err(|error| error.to_string())?;
                    bump(&mut metrics, "nofollow_rejections");
                }
                #[cfg(not(unix))]
                bump(&mut metrics, "symlink_unavailable_on_platform");
            }
        }
        if fs::read(&outside).map_err(|error| error.to_string())? != b"unrelated-host-content" {
            return Err("outside canary changed".into());
        }
    }
    Ok(metrics)
}

pub fn examples() -> Vec<Case> {
    vec![
        Case {
            initial: Vec::new(),
            operations: vec![
                Operation::All { maximum: 1 },
                Operation::Bytes { start: 0, end: 0, maximum: 1 },
                Operation::Lines { first: 1, last: 1, maximum: 1 },
            ],
        },
        Case {
            initial: b"one\r\ntwo\nthree".to_vec(),
            operations: vec![
                Operation::Lines { first: 1, last: 2, maximum: 5 },
                Operation::Lines { first: 2, last: 3, maximum: 100 },
                Operation::Bytes { start: 2, end: 8, maximum: 6 },
            ],
        },
        Case {
            initial: "é👩‍💻\nnext".as_bytes().to_vec(),
            operations: vec![
                Operation::Bytes { start: 1, end: 7, maximum: 6 },
                Operation::Lines { first: 1, last: 2, maximum: 4 },
            ],
        },
        Case {
            initial: vec![0xff, 0, b'\n'],
            operations: vec![
                Operation::All { maximum: 3 },
                Operation::Scan { cancel_before: false },
                Operation::Scan { cancel_before: true },
            ],
        },
        Case {
            initial: b"old".to_vec(),
            operations: vec![
                Operation::Rewrite(b"new".to_vec()),
                Operation::All { maximum: 4 },
                Operation::Symlink,
                Operation::All { maximum: 4 },
            ],
        },
        Case {
            initial: vec![b'x'; 65_537],
            operations: vec![
                Operation::CancelAfterFirstChunk,
                Operation::MutateDuringScan { replace_path: false },
                Operation::MutateDuringScan { replace_path: true },
                Operation::Scan { cancel_before: false },
                Operation::Bytes { start: 65_530, end: 65_535, maximum: 5 },
            ],
        },
    ]
}

pub fn calibration_case() -> Case {
    Case {
        initial: b"abc\nxyz".to_vec(),
        operations: vec![Operation::Bytes { start: 2, end: 7, maximum: 5 }],
    }
}

//! Bounded output retrieval and source-bound matching for owned preview processes.

use peritus_artifact_store::{ArtifactDigest, ArtifactStore};
use peritus_process::OutputStream;
use sha2::{Digest as _, Sha256};

use super::{CommandRuntime, preview_error};
use crate::{PreviewOutputMatch, PreviewOutputMatchSource, PreviewOutputRange, ProductRunnerError};

pub(super) const OUTPUT_SEARCH_CHUNK_BYTES: usize = 64 * 1024;

impl CommandRuntime {
    /// Reads one bounded range from an exact live spool or finalized output artifact.
    ///
    /// # Errors
    /// Returns an error when the process or its durable stream cannot be verified.
    pub fn preview_output_range(
        &self,
        process_id: peritus_types::ProcessId,
        stream: OutputStream,
        offset: u64,
        maximum_bytes: usize,
    ) -> Result<PreviewOutputRange, ProductRunnerError> {
        if maximum_bytes == 0 {
            return Err(preview_error("preview output range size must be positive"));
        }
        let active = self
            .inner
            .state
            .lock()
            .map_err(|_| preview_error("command runtime is poisoned"))?
            .active
            .values()
            .find(|command| command.plan.identity().process_id() == process_id)
            .and_then(|command| command.control.clone());
        if let Some(control) = active {
            let (total_bytes, bytes) = control
                .spooled_stream_range(stream, offset, maximum_bytes)
                .map_err(|error| preview_error(error.to_string()))?;
            return Ok(PreviewOutputRange { total_bytes, digest: None, bytes });
        }
        let terminal = self
            .inner
            .process_store
            .terminal_result(process_id)
            .map_err(|error| preview_error(error.to_string()))?;
        let artifact = terminal
            .artifacts()
            .iter()
            .find(|artifact| artifact.stream() == stream)
            .ok_or_else(|| preview_error("finalized output stream artifact is unavailable"))?;
        if offset > artifact.size() {
            return Err(preview_error("preview output range begins past the finalized stream"));
        }
        let store = ArtifactStore::open(self.inner.artifacts.clone())
            .map_err(|error| preview_error(format!("reopen output artifact store: {error}")))?;
        let mut reader = store
            .open_read(ArtifactDigest::from_sha256(artifact.digest()))
            .map_err(|error| preview_error(format!("open output artifact: {error}")))?;
        let mut bytes = Vec::new();
        while let Some(chunk) = reader
            .read_chunk(64 * 1024)
            .map_err(|error| preview_error(format!("read output artifact: {error}")))?
        {
            let end = chunk
                .offset()
                .saturating_add(u64::try_from(chunk.bytes().len()).unwrap_or(u64::MAX));
            if end > offset {
                let skip =
                    usize::try_from(offset.saturating_sub(chunk.offset())).unwrap_or(usize::MAX);
                let available = &chunk.bytes()[skip.min(chunk.bytes().len())..];
                let remaining = maximum_bytes.saturating_sub(bytes.len());
                bytes.extend_from_slice(&available[..available.len().min(remaining)]);
                if bytes.len() == maximum_bytes {
                    break;
                }
            }
            if end >= artifact.size() {
                break;
            }
        }
        Ok(PreviewOutputRange {
            total_bytes: artifact.size(),
            digest: Some(*artifact.digest().as_bytes()),
            bytes,
        })
    }

    /// Checks the exact process's complete spool or published output artifacts for a string.
    ///
    /// # Errors
    /// Returns an error when the process output evidence cannot be reopened or read.
    pub fn preview_output_contains(
        &self,
        process_id: peritus_types::ProcessId,
        needle: &str,
    ) -> Result<bool, ProductRunnerError> {
        if needle.is_empty() {
            return Ok(true);
        }
        self.preview_output_match(process_id, needle).map(|value| value.is_some())
    }

    /// Searches retained preview output using exact UTF-8 bytes and returns source-bound evidence.
    ///
    /// The live source is the exact process and stream spool observed when this method begins. A
    /// finalized result identifies the immutable stream artifact by its digest. Pipe processes
    /// search stdout then stderr; PTY processes search the combined terminal stream. Searches are
    /// literal and do not perform lossy UTF-8 decoding.
    ///
    /// # Errors
    /// Returns an error when the exact process output source cannot be verified or read.
    pub fn preview_output_match(
        &self,
        process_id: peritus_types::ProcessId,
        needle: &str,
    ) -> Result<Option<PreviewOutputMatch>, ProductRunnerError> {
        if needle.is_empty() {
            return Err(preview_error("preview output search text must not be empty"));
        }
        let active = self
            .inner
            .state
            .lock()
            .map_err(|_| preview_error("command runtime is poisoned"))?
            .active
            .values()
            .find(|command| command.plan.identity().process_id() == process_id)
            .and_then(|command| {
                command.control.clone().map(|control| (control, command.plan.io_mode()))
            });
        if let Some((control, io_mode)) = active {
            return search_live_output(&control, io_mode, process_id, needle);
        }
        let terminal = self
            .inner
            .process_store
            .terminal_result(process_id)
            .map_err(|error| preview_error(error.to_string()))?;
        search_terminal_output(&terminal, &self.inner.artifacts, process_id, needle)
    }

    /// Revalidates exact output evidence against the retained live or finalized source.
    ///
    /// A live-spool observation may be checked against its original spool or, after process
    /// termination, against the same-length prefix of the finalized stream artifact.
    ///
    /// # Errors
    /// Returns an error when source identity, retained length, digest, or matched bytes differ.
    pub fn verify_preview_output_match(
        &self,
        evidence: PreviewOutputMatch,
        needle: &str,
    ) -> Result<(), ProductRunnerError> {
        let process_id = peritus_types::ProcessId::new(evidence.process_id_bytes())
            .map_err(|_| preview_error("preview output evidence process identifier is invalid"))?;
        evidence
            .validate_for_needle(process_id, needle)
            .map_err(|error| preview_error(error.to_owned()))?;
        let stream = evidence.stream().process_stream();
        let active = self
            .inner
            .state
            .lock()
            .map_err(|_| preview_error("command runtime is poisoned"))?
            .active
            .values()
            .find(|command| command.plan.identity().process_id() == process_id)
            .and_then(|command| {
                command.control.clone().map(|control| (control, command.plan.io_mode()))
            });
        if let Some((control, io_mode)) = active {
            let stream_allowed = match io_mode {
                peritus_process::IoMode::Pipes => {
                    matches!(stream, OutputStream::Stdout | OutputStream::Stderr)
                }
                peritus_process::IoMode::Pty(_) => stream == OutputStream::Terminal,
            };
            if !stream_allowed
                || !matches!(evidence.source(), PreviewOutputMatchSource::LiveSpool { .. })
            {
                return Err(preview_error("live output evidence names an unavailable source"));
            }
            let (current_length, _) = control
                .spooled_stream_range(stream, 0, 0)
                .map_err(|error| preview_error(error.to_string()))?;
            if current_length < evidence.observed_stream_bytes() {
                return Err(preview_error("live output spool is shorter than the observed prefix"));
            }
            verify_stream_snapshot(evidence, needle.as_bytes(), |offset, size| {
                let (length, bytes) = control
                    .spooled_stream_range(stream, offset, size)
                    .map_err(|error| error.to_string())?;
                if length < evidence.observed_stream_bytes() {
                    return Err("live output spool became shorter during verification".to_owned());
                }
                Ok(bytes)
            })
            .map_err(preview_error)?;
            return Ok(());
        }

        let terminal = self
            .inner
            .process_store
            .terminal_result(process_id)
            .map_err(|error| preview_error(error.to_string()))?;
        let artifact = terminal
            .artifacts()
            .iter()
            .find(|artifact| artifact.stream() == stream)
            .ok_or_else(|| preview_error("retained output stream artifact is unavailable"))?;
        match evidence.source() {
            PreviewOutputMatchSource::LiveSpool { .. } => {
                if artifact.size() < evidence.observed_stream_bytes() {
                    return Err(preview_error(
                        "final output artifact is shorter than the observed prefix",
                    ));
                }
            }
            PreviewOutputMatchSource::FinalizedArtifact { artifact_digest, .. } => {
                if artifact.size() != evidence.observed_stream_bytes()
                    || *artifact.digest().as_bytes() != artifact_digest
                {
                    return Err(preview_error(
                        "final output artifact identity differs from the evidence",
                    ));
                }
            }
        }
        let store = ArtifactStore::open(self.inner.artifacts.clone())
            .map_err(|error| preview_error(format!("reopen output artifact store: {error}")))?;
        let mut reader = store
            .open_read(ArtifactDigest::from_sha256(artifact.digest()))
            .map_err(|error| preview_error(format!("open output artifact: {error}")))?;
        let mut expected_offset = 0_u64;
        verify_stream_snapshot(evidence, needle.as_bytes(), |offset, size| {
            if offset != expected_offset {
                return Err("output artifact verification offset is inconsistent".to_owned());
            }
            let chunk = reader
                .read_chunk(size)
                .map_err(|error| format!("read output artifact: {error}"))?
                .ok_or_else(|| "output artifact ended before the observed prefix".to_owned())?;
            if chunk.offset() != offset || chunk.bytes().len() > size {
                return Err("output artifact chunk differs from its recorded position".to_owned());
            }
            expected_offset =
                offset.saturating_add(u64::try_from(chunk.bytes().len()).unwrap_or(u64::MAX));
            Ok(chunk.bytes().to_vec())
        })
        .map_err(preview_error)
    }
}

fn search_live_output(
    control: &peritus_process::ProcessControl,
    io_mode: peritus_process::IoMode,
    process_id: peritus_types::ProcessId,
    needle: &str,
) -> Result<Option<PreviewOutputMatch>, ProductRunnerError> {
    let streams: &[OutputStream] = match io_mode {
        peritus_process::IoMode::Pipes => &[OutputStream::Stdout, OutputStream::Stderr],
        peritus_process::IoMode::Pty(_) => &[OutputStream::Terminal],
    };
    for &stream in streams {
        let (observed_stream_bytes, _) = control
            .spooled_stream_range(stream, 0, 0)
            .map_err(|error| preview_error(error.to_string()))?;
        let scan = scan_stream(needle.as_bytes(), observed_stream_bytes, |offset, size| {
            let (current_length, bytes) = control
                .spooled_stream_range(stream, offset, size)
                .map_err(|error| error.to_string())?;
            if current_length < observed_stream_bytes {
                return Err("live output spool became shorter during search".to_owned());
            }
            Ok(bytes)
        })
        .map_err(preview_error)?;
        if let Some(start_byte) = scan.match_start {
            let end_byte = start_byte
                .checked_add(u64::try_from(needle.len()).unwrap_or(u64::MAX))
                .ok_or_else(|| preview_error("preview output match range overflowed"))?;
            return Ok(Some(PreviewOutputMatch {
                stream: stream.into(),
                start_byte,
                end_byte,
                observed_stream_bytes,
                matched_bytes_digest: Sha256::digest(needle.as_bytes()).into(),
                source: PreviewOutputMatchSource::LiveSpool {
                    process_id: *process_id.as_bytes(),
                    observed_prefix_digest: scan.digest,
                },
            }));
        }
    }
    Ok(None)
}

fn search_terminal_output(
    terminal: &peritus_process::TerminalResult,
    artifacts: &peritus_artifact_store::StoreConfig,
    process_id: peritus_types::ProcessId,
    needle: &str,
) -> Result<Option<PreviewOutputMatch>, ProductRunnerError> {
    let store = ArtifactStore::open(artifacts.clone())
        .map_err(|error| preview_error(format!("reopen output artifact store: {error}")))?;
    for artifact in terminal.artifacts() {
        let mut reader = store
            .open_read(ArtifactDigest::from_sha256(artifact.digest()))
            .map_err(|error| preview_error(format!("open output artifact: {error}")))?;
        let mut expected_offset = 0_u64;
        let scan = scan_stream(needle.as_bytes(), artifact.size(), |offset, size| {
            if offset != expected_offset {
                return Err("output artifact search offset is inconsistent".to_owned());
            }
            let chunk = reader
                .read_chunk(size)
                .map_err(|error| format!("read output artifact: {error}"))?
                .ok_or_else(|| "output artifact ended before its recorded size".to_owned())?;
            if chunk.offset() != offset || chunk.bytes().len() > size {
                return Err("output artifact chunk differs from its recorded position".to_owned());
            }
            expected_offset =
                offset.saturating_add(u64::try_from(chunk.bytes().len()).unwrap_or(u64::MAX));
            Ok(chunk.bytes().to_vec())
        })
        .map_err(preview_error)?;
        if scan.digest != *artifact.digest().as_bytes() {
            return Err(preview_error("output artifact digest differs from its retained bytes"));
        }
        if let Some(start_byte) = scan.match_start {
            let end_byte = start_byte
                .checked_add(u64::try_from(needle.len()).unwrap_or(u64::MAX))
                .ok_or_else(|| preview_error("preview output match range overflowed"))?;
            return Ok(Some(PreviewOutputMatch {
                stream: artifact.stream().into(),
                start_byte,
                end_byte,
                observed_stream_bytes: artifact.size(),
                matched_bytes_digest: Sha256::digest(needle.as_bytes()).into(),
                source: PreviewOutputMatchSource::FinalizedArtifact {
                    process_id: *process_id.as_bytes(),
                    artifact_digest: *artifact.digest().as_bytes(),
                },
            }));
        }
    }
    Ok(None)
}

pub(super) fn scan_stream(
    needle: &[u8],
    stream_length: u64,
    mut read: impl FnMut(u64, usize) -> Result<Vec<u8>, String>,
) -> Result<StreamScan, String> {
    let mut offset = 0_u64;
    let mut overlap = Vec::with_capacity(needle.len().saturating_sub(1));
    let mut hasher = Sha256::new();
    let mut match_start = None;
    while offset < stream_length {
        let remaining = stream_length - offset;
        let chunk_length = usize::try_from(
            remaining.min(u64::try_from(OUTPUT_SEARCH_CHUNK_BYTES).unwrap_or(u64::MAX)),
        )
        .map_err(|_| "preview output chunk size cannot be represented".to_owned())?;
        let chunk = read(offset, chunk_length)?;
        if chunk.len() != chunk_length {
            return Err("preview output source ended before its recorded length".to_owned());
        }
        hasher.update(&chunk);
        let overlap_length = overlap.len();
        let mut window = Vec::with_capacity(overlap_length.saturating_add(chunk.len()));
        window.extend_from_slice(&overlap);
        window.extend_from_slice(&chunk);
        if match_start.is_none()
            && let Some(position) = window.windows(needle.len()).position(|bytes| bytes == needle)
        {
            match_start = Some(
                offset
                    .saturating_sub(u64::try_from(overlap_length).unwrap_or(u64::MAX))
                    .saturating_add(u64::try_from(position).unwrap_or(u64::MAX)),
            );
        }
        let keep = needle.len().saturating_sub(1).min(window.len());
        overlap.clear();
        overlap.extend_from_slice(&window[window.len() - keep..]);
        offset = offset
            .checked_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX))
            .ok_or_else(|| "preview output offset overflowed".to_owned())?;
    }
    Ok(StreamScan { match_start, digest: hasher.finalize().into() })
}

fn verify_stream_snapshot(
    evidence: PreviewOutputMatch,
    needle: &[u8],
    mut read: impl FnMut(u64, usize) -> Result<Vec<u8>, String>,
) -> Result<(), String> {
    let mut offset = 0_u64;
    let observed_length = evidence.observed_stream_bytes();
    let mut hasher = Sha256::new();
    let mut matched = Vec::with_capacity(needle.len());
    while offset < observed_length {
        let remaining = observed_length - offset;
        let chunk_length = usize::try_from(
            remaining.min(u64::try_from(OUTPUT_SEARCH_CHUNK_BYTES).unwrap_or(u64::MAX)),
        )
        .map_err(|_| "preview output verification chunk is unrepresentable".to_owned())?;
        let chunk = read(offset, chunk_length)?;
        if chunk.len() != chunk_length {
            return Err("preview output source ended before its observed length".to_owned());
        }
        hasher.update(&chunk);
        let chunk_end = offset
            .checked_add(u64::try_from(chunk.len()).unwrap_or(u64::MAX))
            .ok_or_else(|| "preview output verification offset overflowed".to_owned())?;
        let match_start = evidence.start_byte().max(offset);
        let match_end = evidence.end_byte().min(chunk_end);
        if match_start < match_end {
            let local_start = usize::try_from(match_start - offset)
                .map_err(|_| "preview output match start is unrepresentable".to_owned())?;
            let local_end = usize::try_from(match_end - offset)
                .map_err(|_| "preview output match end is unrepresentable".to_owned())?;
            matched.extend_from_slice(&chunk[local_start..local_end]);
        }
        offset = chunk_end;
    }
    let observed_digest: [u8; 32] = hasher.finalize().into();
    if observed_digest != evidence.observed_source_digest() || matched != needle {
        return Err(
            "preview output evidence digest or matched bytes differ from the source".to_owned()
        );
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct StreamScan {
    pub(super) match_start: Option<u64>,
    pub(super) digest: [u8; 32],
}

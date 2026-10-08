//! Durable recursive literal search with complete source accounting and replayable result pages.

use std::{
    collections::VecDeque,
    fs::{self, File},
    io::{BufRead as _, BufReader, Read as _, Seek as _, SeekFrom, Write as _},
    path::PathBuf,
};

use peritus_patch::WorkspacePath;
use peritus_types::Sha256Digest;
use peritus_workspace::{
    DirectoryExclusionReason, DirectoryItem, FolderIdentity, FolderInspection,
    NativeNameEncoding, RetainedDirectory, WorkspaceEntryKind, WorkspaceError,
};
use serde_json::{Value, json};
use sha2::{Digest as _, Sha256};

use super::{
    super::{access_policy::WorkspaceAccessPolicy, path::traversal_exclusion},
    retained::{
        InspectionError, WorkspaceInspectionOwner, checked_record, digest_hex, encode_hex, seal,
        sync_storage_directory, take_bytes,
    },
};

const SEARCH_RECORD_MAGIC: &[u8; 8] = b"PWSOv001";
const SEARCH_CURSOR_MAGIC: &[u8; 8] = b"PWSCv001";
const SEARCH_PAGE_RECORDS: usize = 32;
const SEARCH_FRAGMENT_BYTES: usize = 8 * 1024;
const COPY_BUFFER_BYTES: usize = 64 * 1024;

impl WorkspaceInspectionOwner {
    pub(crate) fn search_page(
        &self,
        path: Option<&WorkspacePath>,
        query: &str,
        requested_page_items: Option<u64>,
        cursor: Option<&str>,
        access_policy: &WorkspaceAccessPolicy,
    ) -> Result<RetainedSearchPage, InspectionError> {
        if self.is_cancelled() {
            return Err(InspectionError::Cancelled);
        }
        if query.is_empty() {
            return Err(InspectionError::Invalid("search query is empty".to_owned()));
        }
        fs::create_dir_all(&self.storage_root).map_err(InspectionError::Storage)?;
        let request = search_request_binding(path, query);
        let (binding, position) = if let Some(cursor) = cursor {
            let cursor = decode_search_cursor(cursor)?;
            if cursor.request != request {
                return Err(InspectionError::Invalid(
                    "workspace search continuation belongs to another path or query".to_owned(),
                ));
            }
            (cursor.observation, (cursor.index, cursor.offset))
        } else {
            (self.capture_search(path, query, access_policy, request)?, (0, 0))
        };
        let observation = self.read_search_observation(binding)?;
        if observation.request != request {
            return Err(InspectionError::Invalid(
                "workspace search observation belongs to another request".to_owned(),
            ));
        }
        let identity =
            FolderIdentity::observe(&self.workspace_root).map_err(InspectionError::Storage)?;
        if observation.folder != identity.digest() {
            return Err(InspectionError::Invalid(
                "workspace search continuation belongs to another workspace identity".to_owned(),
            ));
        }
        let maximum = requested_page_items
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(SEARCH_PAGE_RECORDS)
            .clamp(1, SEARCH_PAGE_RECORDS);
        self.read_search_page(binding, observation, request, position, maximum)
    }

    fn capture_search(
        &self,
        path: Option<&WorkspacePath>,
        query: &str,
        access_policy: &WorkspaceAccessPolicy,
        request: Sha256Digest,
    ) -> Result<Sha256Digest, InspectionError> {
        let identity =
            FolderIdentity::observe(&self.workspace_root).map_err(InspectionError::Storage)?;
        let inspection =
            FolderInspection::open(&identity).map_err(InspectionError::Workspace)?;
        let candidate =
            tempfile::NamedTempFile::new_in(&self.storage_root).map_err(InspectionError::Storage)?;
        let output = candidate.reopen().map_err(InspectionError::Storage)?;
        let mut writer = RecordWriter::new(output);
        let mut membership = Sha256::new();
        membership.update(b"peritus/workspace-search-membership/v1\0");
        let mut sources = Sha256::new();
        sources.update(b"peritus/workspace-search-sources/v1\0");
        let pattern = SearchPattern::new(query.as_bytes());

        if let Some(path) = path {
            let metadata = inspection.metadata(path).map_err(InspectionError::Workspace)?;
            match metadata.kind() {
                WorkspaceEntryKind::File => self.scan_search_file(
                    &inspection,
                    path,
                    &pattern,
                    &mut sources,
                    &mut writer,
                )?,
                WorkspaceEntryKind::Directory => self.scan_search_directories(
                    &inspection,
                    Some(path.clone()),
                    access_policy,
                    &pattern,
                    &mut membership,
                    &mut sources,
                    &mut writer,
                )?,
            }
        } else {
            self.scan_search_directories(
                &inspection,
                None,
                access_policy,
                &pattern,
                &mut membership,
                &mut sources,
                &mut writer,
            )?;
        }
        let summary = writer.finish()?;
        let observation = SearchObservation {
            folder: identity.digest(),
            request,
            membership: Sha256Digest::new(membership.finalize().into()),
            sources: Sha256Digest::new(sources.finalize().into()),
            records: summary.records,
            matches: summary.matches,
            omissions: summary.omissions,
            body_bytes: summary.bytes,
            body_digest: summary.digest,
        };
        let encoded = observation.encode();
        let binding = peritus_codec::sha256(&encoded);
        let body_path = self.search_body_path(binding);
        match candidate.persist_noclobber(&body_path) {
            Ok(_) => sync_storage_directory(&self.storage_root).map_err(InspectionError::Storage)?,
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                let mut file = File::open(&body_path).map_err(InspectionError::Storage)?;
                verify_search_body(&mut file, &observation)?;
            }
            Err(error) => return Err(InspectionError::Storage(error.error)),
        }
        self.publish_record(binding, &encoded, "search")?;
        Ok(binding)
    }

    #[allow(clippy::too_many_arguments)]
    fn scan_search_directories(
        &self,
        inspection: &FolderInspection,
        start: Option<WorkspacePath>,
        access_policy: &WorkspaceAccessPolicy,
        pattern: &SearchPattern,
        membership: &mut Sha256,
        sources: &mut Sha256,
        writer: &mut RecordWriter,
    ) -> Result<(), InspectionError> {
        let targeted_root = start.clone();
        let mut queue = VecDeque::from([(start, true)]);
        while let Some((directory, required)) = queue.pop_front() {
            if self.is_cancelled() {
                return Err(InspectionError::Cancelled);
            }
            let storage = tempfile::tempfile().map_err(InspectionError::Storage)?;
            let retained = inspection.capture_directory(directory.as_ref(), storage);
            let mut retained = match retained {
                Ok(retained) => retained,
                Err(error) if required => return Err(InspectionError::Workspace(error)),
                Err(error) => {
                    writer.write(&workspace_omission(
                        directory.as_ref().map(WorkspacePath::as_str),
                        "directory_unavailable",
                        &error,
                    ))?;
                    continue;
                }
            };
            let directory_record = retained.observation().to_record();
            membership.update((directory_record.len() as u64).to_be_bytes());
            membership.update(&directory_record);
            while let Some(cursor) = retained.cursor() {
                if self.is_cancelled() {
                    return Err(InspectionError::Cancelled);
                }
                let page = retained
                    .read_page(cursor, u64::MAX)
                    .map_err(InspectionError::Workspace)?;
                for item in page.items() {
                    if let Some(metadata) = item.metadata() {
                        let relative = metadata.path().as_path();
                        if let Some(exclusion) = traversal_exclusion(
                            relative,
                            targeted_root.as_ref().map(WorkspacePath::as_path),
                        ) {
                            writer.write(&json!({
                                "kind": "omission",
                                "path": metadata.path().as_str(),
                                "reason": exclusion.reason(),
                                "detail": exclusion.detail(),
                                "targetable": exclusion.targetable()
                            }))?;
                            continue;
                        }
                        if !access_policy.permits_search_result(relative) {
                            writer.write(&json!({
                                "kind": "omission",
                                "path": Value::Null,
                                "reason": "access_restricted",
                                "detail": "one entry is protected or opaque under the active request policy"
                            }))?;
                            continue;
                        }
                        match metadata.kind() {
                            WorkspaceEntryKind::Directory => {
                                queue.push_back((Some(metadata.path().clone()), false));
                            }
                            WorkspaceEntryKind::File => self.scan_search_file(
                                inspection,
                                metadata.path(),
                                pattern,
                                sources,
                                writer,
                            )?,
                        }
                    } else {
                        writer.write(&directory_omission(directory.as_ref(), item))?;
                    }
                }
            }
        }
        Ok(())
    }

    fn scan_search_file(
        &self,
        inspection: &FolderInspection,
        path: &WorkspacePath,
        pattern: &SearchPattern,
        sources: &mut Sha256,
        writer: &mut RecordWriter,
    ) -> Result<(), InspectionError> {
        if self.is_cancelled() {
            return Err(InspectionError::Cancelled);
        }
        let storage = tempfile::tempfile().map_err(InspectionError::Storage)?;
        let scan = storage.try_clone().map_err(InspectionError::Storage)?;
        let retained =
            inspection.capture_file(path, peritus_workspace::FileReadSelection::all(), storage);
        let retained = match retained {
            Ok(retained) => retained,
            Err(error) => {
                writer.write(&workspace_omission(
                    Some(path.as_str()),
                    "source_unavailable",
                    &error,
                ))?;
                return Ok(());
            }
        };
        let observation = retained.observation().clone();
        let encoded = observation.encode();
        sources.update((encoded.len() as u64).to_be_bytes());
        sources.update(&encoded);
        drop(retained);

        let mut scan = scan;
        if let Err(offset) = validate_utf8(&mut scan) {
            writer.write(&json!({
                "kind": "omission",
                "path": path.as_str(),
                "reason": "invalid_utf8",
                "detail": "source is not valid UTF-8 text",
                "invalid_byte": offset.to_string(),
                "source_bytes": observation.source_bytes().to_string(),
                "source_sha256": digest_hex(observation.source_digest())
            }))?;
            return Ok(());
        }
        let spool = tempfile::tempfile().map_err(InspectionError::Storage)?;
        let mut staged = RecordWriter::new(spool);
        if let Err(error) =
            scan_matching_lines(&mut scan, path, &observation, pattern, &mut staged)
        {
            writer.write(&json!({
                "kind": "omission",
                "path": path.as_str(),
                "reason": "source_io",
                "detail": error.to_string(),
                "source_bytes": observation.source_bytes().to_string(),
                "source_sha256": digest_hex(observation.source_digest())
            }))?;
            return Ok(());
        }
        writer.append(staged)?;
        Ok(())
    }

    fn read_search_observation(
        &self,
        binding: Sha256Digest,
    ) -> Result<SearchObservation, InspectionError> {
        let bytes =
            fs::read(self.record_path(binding, "search")).map_err(InspectionError::Storage)?;
        if peritus_codec::sha256(&bytes) != binding {
            return Err(InspectionError::Invalid(
                "retained workspace search record differs from its handle".to_owned(),
            ));
        }
        SearchObservation::decode(&bytes)
    }

    fn read_search_page(
        &self,
        binding: Sha256Digest,
        observation: SearchObservation,
        request: Sha256Digest,
        position: (u64, u64),
        maximum: usize,
    ) -> Result<RetainedSearchPage, InspectionError> {
        if position.0 > observation.records || position.1 > observation.body_bytes {
            return Err(InspectionError::Invalid(
                "workspace search continuation is beyond the retained result".to_owned(),
            ));
        }
        if observation.records == 0 {
            if position != (0, 0) {
                return Err(InspectionError::Invalid(
                    "completed workspace search has no continuation".to_owned(),
                ));
            }
            return Ok(RetainedSearchPage {
                observation,
                observation_handle: digest_hex(binding),
                start: 0,
                records: Vec::new(),
                replay: None,
                next: None,
            });
        }
        if position.0 == observation.records {
            return Err(InspectionError::Invalid(
                "completed workspace search has no continuation".to_owned(),
            ));
        }
        let path = self.search_body_path(binding);
        let mut file = File::open(&path).map_err(InspectionError::Storage)?;
        verify_search_body(&mut file, &observation)?;
        validate_search_boundary(&mut file, position)?;
        file.seek(SeekFrom::Start(position.1))
            .map_err(InspectionError::Storage)?;
        let mut reader = BufReader::new(&mut file);
        let mut records = Vec::new();
        let mut index = position.0;
        let mut offset = position.1;
        while records.len() < maximum && index < observation.records {
            let mut encoded = Vec::new();
            let read = reader.read_until(b'\n', &mut encoded).map_err(InspectionError::Storage)?;
            if read == 0 || encoded.last() != Some(&b'\n') {
                return Err(InspectionError::Invalid(
                    "retained workspace search record is incomplete".to_owned(),
                ));
            }
            encoded.pop();
            let value = serde_json::from_slice::<Value>(&encoded).map_err(|error| {
                InspectionError::Invalid(format!(
                    "retained workspace search record is invalid JSON: {error}"
                ))
            })?;
            match value.get("kind").and_then(Value::as_str) {
                Some("match" | "omission") => {}
                _ => {
                    return Err(InspectionError::Invalid(
                        "retained workspace search record has no typed kind".to_owned(),
                    ));
                }
            }
            records.push(value);
            index = index.checked_add(1).ok_or_else(|| {
                InspectionError::Invalid("workspace search record counter overflow".to_owned())
            })?;
            offset = offset.checked_add(read as u64).ok_or_else(|| {
                InspectionError::Invalid("workspace search byte counter overflow".to_owned())
            })?;
        }
        drop(reader);
        verify_search_body(&mut file, &observation)?;
        let replay = Some(encode_search_cursor(binding, request, position.0, position.1));
        let next =
            (index < observation.records).then(|| encode_search_cursor(binding, request, index, offset));
        Ok(RetainedSearchPage {
            observation,
            observation_handle: digest_hex(binding),
            start: position.0,
            records,
            replay,
            next,
        })
    }

    fn search_body_path(&self, binding: Sha256Digest) -> PathBuf {
        self.storage_root.join(format!("{}.search", digest_hex(binding)))
    }
}

pub(crate) struct RetainedSearchPage {
    pub(super) observation: SearchObservation,
    pub(super) observation_handle: String,
    pub(super) start: u64,
    pub(super) records: Vec<Value>,
    pub(super) replay: Option<String>,
    pub(super) next: Option<String>,
}

#[derive(Clone, Copy)]
pub(super) struct SearchObservation {
    pub(super) folder: Sha256Digest,
    request: Sha256Digest,
    pub(super) membership: Sha256Digest,
    pub(super) sources: Sha256Digest,
    pub(super) records: u64,
    pub(super) matches: u64,
    pub(super) omissions: u64,
    pub(super) body_bytes: u64,
    body_digest: Sha256Digest,
}

impl SearchObservation {
    fn encode(self) -> Vec<u8> {
        let mut bytes = SEARCH_RECORD_MAGIC.to_vec();
        for digest in [
            self.folder,
            self.request,
            self.membership,
            self.sources,
        ] {
            bytes.extend_from_slice(digest.as_bytes());
        }
        for value in [
            self.records,
            self.matches,
            self.omissions,
            self.body_bytes,
        ] {
            bytes.extend_from_slice(&value.to_be_bytes());
        }
        bytes.extend_from_slice(self.body_digest.as_bytes());
        seal(bytes)
    }

    fn decode(bytes: &[u8]) -> Result<Self, InspectionError> {
        let mut payload = checked_record(bytes, SEARCH_RECORD_MAGIC, "workspace search")?;
        let folder = take_digest(&mut payload, "workspace search folder")?;
        let request = take_digest(&mut payload, "workspace search request")?;
        let membership = take_digest(&mut payload, "workspace search membership")?;
        let sources = take_digest(&mut payload, "workspace search sources")?;
        let records = take_number(&mut payload, "workspace search record count")?;
        let matches = take_number(&mut payload, "workspace search match count")?;
        let omissions = take_number(&mut payload, "workspace search omission count")?;
        let body_bytes = take_number(&mut payload, "workspace search byte count")?;
        let body_digest = take_digest(&mut payload, "workspace search body digest")?;
        if !payload.is_empty()
            || matches.checked_add(omissions) != Some(records)
            || (records == 0) != (body_bytes == 0)
        {
            return Err(InspectionError::Invalid(
                "workspace search observation is inconsistent".to_owned(),
            ));
        }
        Ok(Self {
            folder,
            request,
            membership,
            sources,
            records,
            matches,
            omissions,
            body_bytes,
            body_digest,
        })
    }
}

fn take_digest(bytes: &mut &[u8], kind: &str) -> Result<Sha256Digest, InspectionError> {
    Ok(Sha256Digest::new(
        take_bytes(bytes, 32, kind)?
            .try_into()
            .map_err(|_| InspectionError::Invalid(format!("{kind} is incomplete")))?,
    ))
}

fn take_number(bytes: &mut &[u8], kind: &str) -> Result<u64, InspectionError> {
    Ok(u64::from_be_bytes(
        take_bytes(bytes, 8, kind)?
            .try_into()
            .map_err(|_| InspectionError::Invalid(format!("{kind} is incomplete")))?,
    ))
}

struct SearchCursor {
    observation: Sha256Digest,
    request: Sha256Digest,
    index: u64,
    offset: u64,
}

struct RecordWriter {
    file: File,
    hasher: Sha256,
    bytes: u64,
    records: u64,
    matches: u64,
    omissions: u64,
}

struct RecordSummary {
    bytes: u64,
    records: u64,
    matches: u64,
    omissions: u64,
    digest: Sha256Digest,
}

impl RecordWriter {
    fn new(file: File) -> Self {
        Self {
            file,
            hasher: Sha256::new(),
            bytes: 0,
            records: 0,
            matches: 0,
            omissions: 0,
        }
    }

    fn write(&mut self, value: &Value) -> Result<(), InspectionError> {
        let kind = value.get("kind").and_then(Value::as_str).ok_or_else(|| {
            InspectionError::Invalid("workspace search result has no typed kind".to_owned())
        })?;
        let mut encoded = serde_json::to_vec(value).map_err(|error| {
            InspectionError::Invalid(format!("encode workspace search record: {error}"))
        })?;
        encoded.push(b'\n');
        self.file.write_all(&encoded).map_err(InspectionError::Storage)?;
        self.hasher.update(&encoded);
        self.bytes = self.bytes.checked_add(encoded.len() as u64).ok_or_else(|| {
            InspectionError::Invalid("workspace search byte counter overflow".to_owned())
        })?;
        self.records = self.records.checked_add(1).ok_or_else(|| {
            InspectionError::Invalid("workspace search record counter overflow".to_owned())
        })?;
        match kind {
            "match" => {
                self.matches = self.matches.checked_add(1).ok_or_else(|| {
                    InspectionError::Invalid("workspace search match counter overflow".to_owned())
                })?;
            }
            "omission" => {
                self.omissions = self.omissions.checked_add(1).ok_or_else(|| {
                    InspectionError::Invalid(
                        "workspace search omission counter overflow".to_owned(),
                    )
                })?;
            }
            _ => {
                return Err(InspectionError::Invalid(
                    "workspace search result kind is invalid".to_owned(),
                ));
            }
        }
        Ok(())
    }

    fn append(&mut self, mut other: Self) -> Result<(), InspectionError> {
        other.file.seek(SeekFrom::Start(0)).map_err(InspectionError::Storage)?;
        let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
        loop {
            let read = other.file.read(&mut buffer).map_err(InspectionError::Storage)?;
            if read == 0 {
                break;
            }
            self.file.write_all(&buffer[..read]).map_err(InspectionError::Storage)?;
            self.hasher.update(&buffer[..read]);
        }
        self.bytes = self.bytes.checked_add(other.bytes).ok_or_else(|| {
            InspectionError::Invalid("workspace search byte counter overflow".to_owned())
        })?;
        self.records = self.records.checked_add(other.records).ok_or_else(|| {
            InspectionError::Invalid("workspace search record counter overflow".to_owned())
        })?;
        self.matches = self.matches.checked_add(other.matches).ok_or_else(|| {
            InspectionError::Invalid("workspace search match counter overflow".to_owned())
        })?;
        self.omissions = self.omissions.checked_add(other.omissions).ok_or_else(|| {
            InspectionError::Invalid("workspace search omission counter overflow".to_owned())
        })?;
        Ok(())
    }

    fn finish(self) -> Result<RecordSummary, InspectionError> {
        self.file.sync_all().map_err(InspectionError::Storage)?;
        Ok(RecordSummary {
            bytes: self.bytes,
            records: self.records,
            matches: self.matches,
            omissions: self.omissions,
            digest: Sha256Digest::new(self.hasher.finalize().into()),
        })
    }
}

struct SearchPattern {
    bytes: Vec<u8>,
    prefix: Vec<usize>,
}

impl SearchPattern {
    fn new(bytes: &[u8]) -> Self {
        let mut prefix = vec![0; bytes.len()];
        let mut matched = 0;
        for index in 1..bytes.len() {
            while matched > 0 && bytes[index] != bytes[matched] {
                matched = prefix[matched - 1];
            }
            if bytes[index] == bytes[matched] {
                matched += 1;
            }
            prefix[index] = matched;
        }
        Self { bytes: bytes.to_vec(), prefix }
    }

    fn contains(&self, file: &mut File, length: u64) -> Result<bool, std::io::Error> {
        file.seek(SeekFrom::Start(0))?;
        let mut remaining = length;
        let mut matched = 0_usize;
        let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
        while remaining > 0 {
            let capacity = usize::try_from(remaining.min(buffer.len() as u64)).unwrap_or(buffer.len());
            let read = file.read(&mut buffer[..capacity])?;
            if read == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    "retained search line ended early",
                ));
            }
            for byte in &buffer[..read] {
                while matched > 0 && *byte != self.bytes[matched] {
                    matched = self.prefix[matched - 1];
                }
                if *byte == self.bytes[matched] {
                    matched += 1;
                    if matched == self.bytes.len() {
                        return Ok(true);
                    }
                }
            }
            remaining -= read as u64;
        }
        Ok(false)
    }
}

fn scan_matching_lines(
    source: &mut File,
    path: &WorkspacePath,
    observation: &peritus_workspace::InspectedSelection,
    pattern: &SearchPattern,
    output: &mut RecordWriter,
) -> Result<(), std::io::Error> {
    source.seek(SeekFrom::Start(0))?;
    let mut line_file = tempfile::tempfile()?;
    let mut line_bytes = 0_u64;
    let mut line_start = 0_u64;
    let mut source_offset = 0_u64;
    let mut line_number = 1_u64;
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    loop {
        let read = source.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let mut segment = 0;
        for index in 0..read {
            if buffer[index] != b'\n' {
                continue;
            }
            line_file.write_all(&buffer[segment..index])?;
            line_bytes += (index - segment) as u64;
            finish_search_line(
                &mut line_file,
                line_bytes,
                line_start,
                line_number,
                path,
                observation,
                pattern,
                output,
            )
            .map_err(inspection_io)?;
            line_file.set_len(0)?;
            line_file.seek(SeekFrom::Start(0))?;
            line_bytes = 0;
            line_start = source_offset + index as u64 + 1;
            line_number = line_number.checked_add(1).ok_or_else(|| {
                std::io::Error::other("workspace search line counter overflow")
            })?;
            segment = index + 1;
        }
        line_file.write_all(&buffer[segment..read])?;
        line_bytes += (read - segment) as u64;
        source_offset += read as u64;
    }
    if line_bytes > 0 {
        finish_search_line(
            &mut line_file,
            line_bytes,
            line_start,
            line_number,
            path,
            observation,
            pattern,
            output,
        )
        .map_err(inspection_io)?;
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn finish_search_line(
    file: &mut File,
    bytes: u64,
    line_start: u64,
    line_number: u64,
    path: &WorkspacePath,
    observation: &peritus_workspace::InspectedSelection,
    pattern: &SearchPattern,
    output: &mut RecordWriter,
) -> Result<(), InspectionError> {
    let logical = if bytes > 0 {
        file.seek(SeekFrom::Start(bytes - 1)).map_err(InspectionError::Storage)?;
        let mut last = [0_u8; 1];
        file.read_exact(&mut last).map_err(InspectionError::Storage)?;
        bytes - u64::from(last[0] == b'\r')
    } else {
        0
    };
    if !pattern.contains(file, logical).map_err(InspectionError::Storage)? {
        return Ok(());
    }
    file.seek(SeekFrom::Start(0)).map_err(InspectionError::Storage)?;
    let mut fragment_start = 0_u64;
    while fragment_start < logical {
        let maximum = usize::try_from((logical - fragment_start).min(SEARCH_FRAGMENT_BYTES as u64))
            .map_err(|_| {
                InspectionError::Invalid("workspace search fragment is not representable".to_owned())
            })?;
        let mut fragment = vec![0_u8; maximum];
        file.read_exact(&mut fragment).map_err(InspectionError::Storage)?;
        if let Err(error) = std::str::from_utf8(&fragment) {
            if error.error_len().is_some() || error.valid_up_to() == 0 {
                return Err(InspectionError::Invalid(
                    "validated workspace search source changed encoding".to_owned(),
                ));
            }
            let trailing = fragment.len() - error.valid_up_to();
            file.seek(SeekFrom::Current(-(trailing as i64)))
                .map_err(InspectionError::Storage)?;
            fragment.truncate(error.valid_up_to());
        }
        let text = std::str::from_utf8(&fragment)
            .map_err(|_| {
                InspectionError::Invalid(
                    "validated workspace search fragment is not UTF-8".to_owned(),
                )
            })?
            .to_owned();
        let fragment_end = fragment_start + fragment.len() as u64;
        output.write(&json!({
            "kind": "match",
            "path": path.as_str(),
            "line": line_number,
            "text": text,
            "line_start_byte": line_start.to_string(),
            "line_end_byte": (line_start + logical).to_string(),
            "fragment_start_byte": (line_start + fragment_start).to_string(),
            "fragment_end_byte": (line_start + fragment_end).to_string(),
            "fragment_start_column_bytes": fragment_start.to_string(),
            "fragment_end_column_bytes": fragment_end.to_string(),
            "continued_from_previous": fragment_start != 0,
            "continues": fragment_end < logical,
            "line_complete": fragment_start == 0 && fragment_end == logical,
            "source_bytes": observation.source_bytes().to_string(),
            "source_sha256": digest_hex(observation.source_digest())
        }))?;
        fragment_start = fragment_end;
    }
    Ok(())
}

fn validate_utf8(file: &mut File) -> Result<(), u64> {
    file.seek(SeekFrom::Start(0)).map_err(|_| 0_u64)?;
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    let mut carry = Vec::new();
    let mut total = 0_u64;
    loop {
        let read = file.read(&mut buffer).map_err(|_| total)?;
        if read == 0 {
            break;
        }
        let base = total.saturating_sub(carry.len() as u64);
        let mut combined = std::mem::take(&mut carry);
        combined.extend_from_slice(&buffer[..read]);
        match std::str::from_utf8(&combined) {
            Ok(_) => {}
            Err(error) if error.error_len().is_none() => {
                carry.extend_from_slice(&combined[error.valid_up_to()..]);
            }
            Err(error) => return Err(base + error.valid_up_to() as u64),
        }
        total += read as u64;
    }
    if carry.is_empty() {
        Ok(())
    } else {
        Err(total - carry.len() as u64)
    }
}

fn directory_omission(parent: Option<&WorkspacePath>, item: &DirectoryItem) -> Value {
    let path = item.name().native_name().ok().map(|name| {
        parent.map_or_else(
            || PathBuf::from(&name),
            |parent| parent.as_path().join(&name),
        )
    });
    let reason = item.exclusion().map_or("unavailable", DirectoryExclusionReason::as_str);
    json!({
        "kind": "omission",
        "path": path.map(|path| path.to_string_lossy().into_owned()),
        "reason": reason,
        "detail": "directory member cannot be inspected as an ordinary file or directory",
        "name": item.name().display_name(),
        "native_encoding": match item.name().encoding() {
            NativeNameEncoding::UnixBytes => "unix_bytes",
            NativeNameEncoding::WindowsWide => "windows_utf16_be",
            NativeNameEncoding::Utf8 => "utf8",
        },
        "native_units": encode_hex(item.name().encoded_bytes())
    })
}

fn workspace_omission(path: Option<&str>, reason: &str, error: &WorkspaceError) -> Value {
    json!({
        "kind": "omission",
        "path": path,
        "reason": reason,
        "workspace_code": error.code().as_str(),
        "recovery": match error.recovery() {
            peritus_workspace::RecoveryClass::CorrectRequest => "correct_request",
            peritus_workspace::RecoveryClass::Reauthorize => "reauthorize",
            peritus_workspace::RecoveryClass::Reobserve => "reobserve",
            peritus_workspace::RecoveryClass::Reconcile => "reconcile",
            peritus_workspace::RecoveryClass::Quarantine => "quarantine",
        },
        "detail": error.detail()
    })
}

fn search_request_binding(path: Option<&WorkspacePath>, query: &str) -> Sha256Digest {
    let mut bytes = b"peritus/workspace-search-request/v1\0".to_vec();
    match path {
        Some(path) => {
            bytes.push(1);
            bytes.extend_from_slice(&(path.as_str().len() as u64).to_be_bytes());
            bytes.extend_from_slice(path.as_str().as_bytes());
        }
        None => bytes.push(0),
    }
    bytes.extend_from_slice(&(query.len() as u64).to_be_bytes());
    bytes.extend_from_slice(query.as_bytes());
    peritus_codec::sha256(&bytes)
}

fn encode_search_cursor(
    observation: Sha256Digest,
    request: Sha256Digest,
    index: u64,
    offset: u64,
) -> String {
    let mut bytes = SEARCH_CURSOR_MAGIC.to_vec();
    bytes.extend_from_slice(observation.as_bytes());
    bytes.extend_from_slice(request.as_bytes());
    bytes.extend_from_slice(&index.to_be_bytes());
    bytes.extend_from_slice(&offset.to_be_bytes());
    encode_hex(&seal(bytes))
}

fn decode_search_cursor(value: &str) -> Result<SearchCursor, InspectionError> {
    let bytes = decode_hex_cursor(value)?;
    let mut payload = checked_record(&bytes, SEARCH_CURSOR_MAGIC, "workspace search cursor")?;
    let digest = |payload: &mut &[u8], kind: &str| -> Result<Sha256Digest, InspectionError> {
        Ok(Sha256Digest::new(
            take_bytes(payload, 32, kind)?
                .try_into()
                .map_err(|_| InspectionError::Invalid(format!("{kind} is incomplete")))?,
        ))
    };
    let observation = digest(&mut payload, "workspace search observation")?;
    let request = digest(&mut payload, "workspace search request")?;
    let number = |payload: &mut &[u8], kind: &str| -> Result<u64, InspectionError> {
        Ok(u64::from_be_bytes(
            take_bytes(payload, 8, kind)?
                .try_into()
                .map_err(|_| InspectionError::Invalid(format!("{kind} is incomplete")))?,
        ))
    };
    let index = number(&mut payload, "workspace search record index")?;
    let offset = number(&mut payload, "workspace search byte offset")?;
    if !payload.is_empty() {
        return Err(InspectionError::Invalid(
            "workspace search cursor has trailing data".to_owned(),
        ));
    }
    Ok(SearchCursor { observation, request, index, offset })
}

fn decode_hex_cursor(value: &str) -> Result<Vec<u8>, InspectionError> {
    if !value.len().is_multiple_of(2) {
        return Err(InspectionError::Invalid(
            "workspace search cursor is not hexadecimal".to_owned(),
        ));
    }
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte| match byte {
                b'0'..=b'9' => Some(byte - b'0'),
                b'a'..=b'f' => Some(byte - b'a' + 10),
                b'A'..=b'F' => Some(byte - b'A' + 10),
                _ => None,
            };
            let high = digit(pair[0]).ok_or_else(|| {
                InspectionError::Invalid(
                    "workspace search cursor is not hexadecimal".to_owned(),
                )
            })?;
            let low = digit(pair[1]).ok_or_else(|| {
                InspectionError::Invalid(
                    "workspace search cursor is not hexadecimal".to_owned(),
                )
            })?;
            Ok((high << 4) | low)
        })
        .collect()
}

fn verify_search_body(
    file: &mut File,
    observation: &SearchObservation,
) -> Result<(), InspectionError> {
    if file.metadata().map_err(InspectionError::Storage)?.len() != observation.body_bytes {
        return Err(InspectionError::Invalid(
            "retained workspace search body size changed".to_owned(),
        ));
    }
    file.seek(SeekFrom::Start(0)).map_err(InspectionError::Storage)?;
    let mut hasher = Sha256::new();
    let mut count = 0_u64;
    let mut buffer = vec![0_u8; COPY_BUFFER_BYTES];
    loop {
        let read = file.read(&mut buffer).map_err(InspectionError::Storage)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
        count = count.checked_add(read as u64).ok_or_else(|| {
            InspectionError::Invalid("workspace search byte counter overflow".to_owned())
        })?;
    }
    if count != observation.body_bytes
        || Sha256Digest::new(hasher.finalize().into()) != observation.body_digest
    {
        return Err(InspectionError::Invalid(
            "retained workspace search body differs from its observation".to_owned(),
        ));
    }
    Ok(())
}

fn validate_search_boundary(
    file: &mut File,
    position: (u64, u64),
) -> Result<(), InspectionError> {
    file.seek(SeekFrom::Start(0)).map_err(InspectionError::Storage)?;
    let mut reader = BufReader::new(file);
    let mut index = 0_u64;
    let mut offset = 0_u64;
    while offset < position.1 {
        let mut encoded = Vec::new();
        let read = reader.read_until(b'\n', &mut encoded).map_err(InspectionError::Storage)?;
        if read == 0 || encoded.last() != Some(&b'\n') {
            return Err(InspectionError::Invalid(
                "workspace search cursor is not a retained record boundary".to_owned(),
            ));
        }
        index = index.checked_add(1).ok_or_else(|| {
            InspectionError::Invalid("workspace search record counter overflow".to_owned())
        })?;
        offset = offset.checked_add(read as u64).ok_or_else(|| {
            InspectionError::Invalid("workspace search byte counter overflow".to_owned())
        })?;
    }
    if (index, offset) != position {
        return Err(InspectionError::Invalid(
            "workspace search cursor record index and byte boundary disagree".to_owned(),
        ));
    }
    Ok(())
}

fn inspection_io(error: InspectionError) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

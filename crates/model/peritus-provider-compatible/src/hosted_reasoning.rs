//! Closed hosted reasoning fields preserved for exact subsequent tool-result replay.

use std::collections::BTreeMap;
use std::io;

use crate::error;
use peritus_provider_core::{ProviderCoreError, hosted::HostedService};
use peritus_types::Sha256Digest;
use serde::Serialize;
use serde_json::{Map, Value};

const DETAILS: &str = "reasoning_details";
const REPLAY_FRAGMENT_SOURCE_BYTES: usize = 2_048;

pub fn accepts(service: HostedService, field: &str) -> bool {
    match service {
        HostedService::OpenRouter => field == "reasoning" || field == DETAILS,
        HostedService::Groq => field == "reasoning",
        HostedService::Together => matches!(field, "reasoning" | "reasoning_content"),
        HostedService::OpenCodeZen
        | HostedService::OpenCodeGo
        | HostedService::Fireworks
        | HostedService::DeepSeek => field == "reasoning_content",
    }
}

pub struct HostedReasoning {
    fields: Map<String, Value>,
    encoded_len: usize,
    detail_lengths: Vec<usize>,
    detail_indexes: BTreeMap<u64, usize>,
    detail_ids: BTreeMap<Sha256Digest, Vec<usize>>,
}

impl Default for HostedReasoning {
    fn default() -> Self {
        Self {
            fields: Map::new(),
            encoded_len: 2,
            detail_lengths: Vec::new(),
            detail_indexes: BTreeMap::new(),
            detail_ids: BTreeMap::new(),
        }
    }
}

impl HostedReasoning {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    pub fn append(
        &mut self,
        name: &str,
        value: &Value,
        maximum: usize,
    ) -> Result<(), ProviderCoreError> {
        if value.is_null() {
            return Ok(());
        }
        if name == DETAILS {
            self.append_details(value, maximum)
        } else {
            self.append_field_text(name, value, maximum)
        }
    }

    pub fn replay(
        self,
        service: HostedService,
        maximum: usize,
    ) -> Result<ReplayBytes, ProviderCoreError> {
        let service_len = encoded_len(service.name())?;
        let fields_member = member_len("fields", self.encoded_len)?;
        let service_member = member_len("service", service_len)?;
        let expected = checked_sum(&[2, fields_member, 1, service_member])?;
        if expected > maximum {
            return Err(error::limit("reasoning replay exceeded its aggregate bound"));
        }
        let mut wrapper = Map::new();
        wrapper.insert("fields".to_owned(), Value::Object(self.fields));
        wrapper.insert("service".to_owned(), Value::String(service.name().to_owned()));
        Ok(ReplayBytes {
            tasks: vec![ReplayTask::Value(Value::Object(wrapper))],
            expected,
            emitted: 0,
        })
    }

    fn append_field_text(
        &mut self,
        name: &str,
        value: &Value,
        maximum: usize,
    ) -> Result<(), ProviderCoreError> {
        let text = value
            .as_str()
            .ok_or_else(|| error::malformed("reasoning delta is not a string"))?;
        match self.fields.get(name) {
            None => {
                let value_len = encoded_len(text)?;
                let added = member_addition(self.fields.len(), name, value_len)?;
                let next = adjusted_len(self.encoded_len, 0, added, maximum)?;
                self.fields.insert(name.to_owned(), Value::String(text.to_owned()));
                self.encoded_len = next;
            }
            Some(Value::Null) => {
                let value_len = encoded_len(text)?;
                let next = adjusted_len(self.encoded_len, 4, value_len, maximum)?;
                self.fields.insert(name.to_owned(), Value::String(text.to_owned()));
                self.encoded_len = next;
            }
            Some(Value::String(_)) => {
                let added = string_content_len(text)?;
                let next = adjusted_len(self.encoded_len, 0, added, maximum)?;
                let Some(Value::String(stored)) = self.fields.get_mut(name) else {
                    return Err(error::malformed("reasoning delta storage changed type"));
                };
                stored.push_str(text);
                self.encoded_len = next;
            }
            Some(_) => return Err(error::malformed("reasoning delta changed type")),
        }
        Ok(())
    }

    fn append_details(
        &mut self,
        value: &Value,
        maximum: usize,
    ) -> Result<(), ProviderCoreError> {
        let incoming = value
            .as_array()
            .ok_or_else(|| error::malformed("reasoning details are not an array"))?;
        self.ensure_details(maximum)?;
        for detail in incoming {
            let object = detail
                .as_object()
                .ok_or_else(|| error::malformed("reasoning detail is not an object"))?;
            if object.keys().any(|name| {
                !matches!(
                    name.as_str(),
                    "type"
                        | "id"
                        | "format"
                        | "index"
                        | "text"
                        | "summary"
                        | "data"
                        | "signature"
                )
            }) {
                return Err(error::malformed("reasoning detail contains an unmapped field"));
            }
            if let Some(position) = self.find_detail(object)? {
                self.merge_detail(position, object, maximum)?;
            } else {
                self.push_detail(detail, object, maximum)?;
            }
        }
        Ok(())
    }

    fn ensure_details(&mut self, maximum: usize) -> Result<(), ProviderCoreError> {
        if let Some(stored) = self.fields.get(DETAILS) {
            if stored.is_array() {
                return Ok(());
            }
            return Err(error::malformed("reasoning detail storage changed type"));
        }
        let added = member_addition(self.fields.len(), DETAILS, 2)?;
        let next = adjusted_len(self.encoded_len, 0, added, maximum)?;
        self.fields.insert(DETAILS.to_owned(), Value::Array(Vec::new()));
        self.encoded_len = next;
        Ok(())
    }

    fn find_detail(
        &self,
        object: &Map<String, Value>,
    ) -> Result<Option<usize>, ProviderCoreError> {
        let mut prior = object
            .get("index")
            .and_then(Value::as_u64)
            .and_then(|index| self.detail_indexes.get(&index).copied());
        if let Some(id) = object.get("id").and_then(Value::as_str) {
            let digest = peritus_codec::sha256(id.as_bytes());
            if let Some(candidates) = self.detail_ids.get(&digest) {
                for position in candidates {
                    if self
                        .detail_object(*position)?
                        .get("id")
                        .and_then(Value::as_str)
                        == Some(id)
                    {
                        prior = Some(prior.map_or(*position, |known| known.min(*position)));
                    }
                }
            }
        }
        Ok(prior)
    }

    fn push_detail(
        &mut self,
        detail: &Value,
        object: &Map<String, Value>,
        maximum: usize,
    ) -> Result<(), ProviderCoreError> {
        let detail_len = encoded_len(detail)?;
        let comma = usize::from(!self.detail_lengths.is_empty());
        let added = detail_len
            .checked_add(comma)
            .ok_or_else(|| error::limit("reasoning state size overflowed"))?;
        let next = adjusted_len(self.encoded_len, 0, added, maximum)?;
        let position = self.detail_lengths.len();
        let stored = self.details_mut()?;
        debug_assert_eq!(stored.len(), position);
        stored.push(detail.clone());
        self.detail_lengths.push(detail_len);
        self.encoded_len = next;
        self.record_identities(position, object);
        Ok(())
    }

    fn merge_detail(
        &mut self,
        position: usize,
        object: &Map<String, Value>,
        maximum: usize,
    ) -> Result<(), ProviderCoreError> {
        for (key, value) in object {
            if value.is_null() {
                continue;
            }
            if matches!(key.as_str(), "text" | "summary" | "data" | "signature") {
                self.merge_detail_text(position, key, value, maximum)?;
                continue;
            }
            let mutation = match self.detail_object(position)?.get(key) {
                None => DetailMutation::Insert,
                Some(Value::Null) => DetailMutation::Replace,
                Some(old) if old != value => {
                    return Err(error::malformed("reasoning detail identity changed"));
                }
                Some(_) => continue,
            };
            let value_len = encoded_len(value)?;
            let (removed, added) = match mutation {
                DetailMutation::Insert => (
                    0,
                    member_addition(self.detail_object(position)?.len(), key, value_len)?,
                ),
                DetailMutation::Replace => (4, value_len),
                DetailMutation::Append => {
                    return Err(error::malformed("reasoning detail mutation changed"));
                }
            };
            let (detail_len, total_len) =
                self.prospective_detail_lengths(position, removed, added, maximum)?;
            self.detail_object_mut(position)?.insert(key.clone(), value.clone());
            self.detail_lengths[position] = detail_len;
            self.encoded_len = total_len;
            self.record_identity(position, key, value);
        }
        Ok(())
    }

    fn merge_detail_text(
        &mut self,
        position: usize,
        key: &str,
        value: &Value,
        maximum: usize,
    ) -> Result<(), ProviderCoreError> {
        let text = value
            .as_str()
            .ok_or_else(|| error::malformed("reasoning delta is not a string"))?;
        let mutation = match self.detail_object(position)?.get(key) {
            None => DetailMutation::Insert,
            Some(Value::Null) => DetailMutation::Replace,
            Some(Value::String(_)) => DetailMutation::Append,
            Some(_) => return Err(error::malformed("reasoning delta changed type")),
        };
        let value_len = encoded_len(text)?;
        let (removed, added) = match mutation {
            DetailMutation::Insert => (
                0,
                member_addition(self.detail_object(position)?.len(), key, value_len)?,
            ),
            DetailMutation::Replace => (4, value_len),
            DetailMutation::Append => (0, value_len.saturating_sub(2)),
        };
        let (detail_len, total_len) =
            self.prospective_detail_lengths(position, removed, added, maximum)?;
        let stored = self.detail_object_mut(position)?;
        match mutation {
            DetailMutation::Insert | DetailMutation::Replace => {
                stored.insert(key.to_owned(), Value::String(text.to_owned()));
            }
            DetailMutation::Append => {
                let Some(Value::String(current)) = stored.get_mut(key) else {
                    return Err(error::malformed("reasoning delta storage changed type"));
                };
                current.push_str(text);
            }
        }
        self.detail_lengths[position] = detail_len;
        self.encoded_len = total_len;
        Ok(())
    }

    fn prospective_detail_lengths(
        &self,
        position: usize,
        removed: usize,
        added: usize,
        maximum: usize,
    ) -> Result<(usize, usize), ProviderCoreError> {
        let current = *self
            .detail_lengths
            .get(position)
            .ok_or_else(|| error::malformed("reasoning detail index was unavailable"))?;
        let detail = adjusted_len(current, removed, added, usize::MAX)?;
        let total = adjusted_len(self.encoded_len, removed, added, maximum)?;
        Ok((detail, total))
    }

    fn record_identities(&mut self, position: usize, object: &Map<String, Value>) {
        for key in ["index", "id"] {
            if let Some(value) = object.get(key) {
                self.record_identity(position, key, value);
            }
        }
    }

    fn record_identity(&mut self, position: usize, key: &str, value: &Value) {
        if key == "index" {
            if let Some(index) = value.as_u64() {
                self.detail_indexes
                    .entry(index)
                    .and_modify(|known| *known = (*known).min(position))
                    .or_insert(position);
            }
        } else if key == "id"
            && let Some(id) = value.as_str()
        {
            let positions = self.detail_ids.entry(peritus_codec::sha256(id.as_bytes())).or_default();
            if !positions.contains(&position) {
                positions.push(position);
            }
        }
    }

    fn details_mut(&mut self) -> Result<&mut Vec<Value>, ProviderCoreError> {
        self.fields
            .get_mut(DETAILS)
            .and_then(Value::as_array_mut)
            .ok_or_else(|| error::malformed("reasoning detail storage changed type"))
    }

    fn detail_object(&self, position: usize) -> Result<&Map<String, Value>, ProviderCoreError> {
        self.fields
            .get(DETAILS)
            .and_then(Value::as_array)
            .and_then(|details| details.get(position))
            .and_then(Value::as_object)
            .ok_or_else(|| error::malformed("reasoning detail storage changed type"))
    }

    fn detail_object_mut(
        &mut self,
        position: usize,
    ) -> Result<&mut Map<String, Value>, ProviderCoreError> {
        self.fields
            .get_mut(DETAILS)
            .and_then(Value::as_array_mut)
            .and_then(|details| details.get_mut(position))
            .and_then(Value::as_object_mut)
            .ok_or_else(|| error::malformed("reasoning detail storage changed type"))
    }
}

#[derive(Clone, Copy)]
enum DetailMutation {
    Insert,
    Replace,
    Append,
}

pub struct ReplayBytes {
    tasks: Vec<ReplayTask>,
    expected: usize,
    emitted: usize,
}

impl ReplayBytes {
    pub fn next_chunk(&mut self, maximum: usize) -> Result<Option<Vec<u8>>, ProviderCoreError> {
        if maximum == 0 {
            return Err(error::limit("reasoning replay fragment bound was zero"));
        }
        let mut output = Vec::with_capacity(maximum);
        while output.len() < maximum {
            let Some(task) = self.tasks.pop() else { break };
            match task {
                ReplayTask::Value(value) => self.expand_value(value)?,
                ReplayTask::Array { mut values, first } => {
                    if let Some(value) = values.next() {
                        self.tasks.push(ReplayTask::Array { values, first: false });
                        self.tasks.push(ReplayTask::Value(value));
                        if !first {
                            self.push_static(b",");
                        }
                    } else {
                        self.push_static(b"]");
                    }
                }
                ReplayTask::Object { mut values, first } => {
                    if let Some((key, value)) = values.next() {
                        self.tasks.push(ReplayTask::Object { values, first: false });
                        self.tasks.push(ReplayTask::Value(value));
                        self.push_static(b":");
                        self.push_string(key);
                        if !first {
                            self.push_static(b",");
                        }
                    } else {
                        self.push_static(b"}");
                    }
                }
                ReplayTask::Static { bytes, offset } => {
                    let next = copy_piece(&mut output, maximum, bytes, offset, bytes.len());
                    if next < bytes.len() {
                        self.tasks.push(ReplayTask::Static { bytes, offset: next });
                    }
                }
                ReplayTask::Owned { bytes, offset, end } => {
                    let next = copy_piece(&mut output, maximum, &bytes, offset, end);
                    if next < end {
                        self.tasks.push(ReplayTask::Owned { bytes, offset: next, end });
                    }
                }
                ReplayTask::StringContent { value, offset } => {
                    self.expand_string(value, offset, maximum - output.len())?;
                }
            }
        }
        let next = self
            .emitted
            .checked_add(output.len())
            .ok_or_else(|| error::limit("reasoning replay size overflowed"))?;
        if next > self.expected {
            return Err(error::malformed("reasoning replay exceeded its canonical size"));
        }
        self.emitted = next;
        if output.is_empty() {
            if self.emitted != self.expected {
                return Err(error::malformed("reasoning replay canonical size changed"));
            }
            return Ok(None);
        }
        Ok(Some(output))
    }

    fn expand_value(&mut self, value: Value) -> Result<(), ProviderCoreError> {
        match value {
            Value::Null => self.push_static(b"null"),
            Value::Bool(true) => self.push_static(b"true"),
            Value::Bool(false) => self.push_static(b"false"),
            Value::Number(number) => {
                let bytes = serde_json::to_vec(&Value::Number(number))
                    .map_err(|_| error::malformed("reasoning replay serialization failed"))?;
                let end = bytes.len();
                self.tasks.push(ReplayTask::Owned { bytes, offset: 0, end });
            }
            Value::String(value) => self.push_string(value),
            Value::Array(values) => {
                self.tasks
                    .push(ReplayTask::Array { values: values.into_iter(), first: true });
                self.push_static(b"[");
            }
            Value::Object(values) => {
                self.tasks
                    .push(ReplayTask::Object { values: values.into_iter(), first: true });
                self.push_static(b"{");
            }
        }
        Ok(())
    }

    fn expand_string(
        &mut self,
        value: String,
        offset: usize,
        available: usize,
    ) -> Result<(), ProviderCoreError> {
        if offset == value.len() {
            return Ok(());
        }
        let source_bytes = available.max(1).min(REPLAY_FRAGMENT_SOURCE_BYTES);
        let mut end = offset.saturating_add(source_bytes).min(value.len());
        while end > offset && !value.is_char_boundary(end) {
            end -= 1;
        }
        if end == offset {
            let width = value[offset..]
                .chars()
                .next()
                .ok_or_else(|| error::malformed("reasoning replay string cursor was invalid"))?
                .len_utf8();
            end = offset
                .checked_add(width)
                .ok_or_else(|| error::limit("reasoning replay string cursor overflowed"))?;
        }
        let encoded = serde_json::to_vec(&value[offset..end])
            .map_err(|_| error::malformed("reasoning replay serialization failed"))?;
        if encoded.len() < 2 {
            return Err(error::malformed("reasoning replay string encoding was invalid"));
        }
        if end < value.len() {
            self.tasks.push(ReplayTask::StringContent { value, offset: end });
        }
        let encoded_end = encoded.len() - 1;
        self.tasks.push(ReplayTask::Owned { bytes: encoded, offset: 1, end: encoded_end });
        Ok(())
    }

    fn push_string(&mut self, value: String) {
        self.push_static(b"\"");
        if !value.is_empty() {
            self.tasks.push(ReplayTask::StringContent { value, offset: 0 });
        }
        self.push_static(b"\"");
    }

    fn push_static(&mut self, bytes: &'static [u8]) {
        self.tasks.push(ReplayTask::Static { bytes, offset: 0 });
    }
}

enum ReplayTask {
    Value(Value),
    Array {
        values: std::vec::IntoIter<Value>,
        first: bool,
    },
    Object {
        values: serde_json::map::IntoIter,
        first: bool,
    },
    Static {
        bytes: &'static [u8],
        offset: usize,
    },
    Owned {
        bytes: Vec<u8>,
        offset: usize,
        end: usize,
    },
    StringContent {
        value: String,
        offset: usize,
    },
}

fn copy_piece(
    output: &mut Vec<u8>,
    maximum: usize,
    source: &[u8],
    offset: usize,
    end: usize,
) -> usize {
    let amount = (maximum - output.len()).min(end - offset);
    let next = offset + amount;
    output.extend_from_slice(&source[offset..next]);
    next
}

fn encoded_len<T: ?Sized + Serialize>(value: &T) -> Result<usize, ProviderCoreError> {
    let mut writer = CountingWriter { length: 0, overflowed: false };
    if serde_json::to_writer(&mut writer, value).is_err() {
        return Err(if writer.overflowed {
            error::limit("reasoning state size overflowed")
        } else {
            error::malformed("reasoning state serialization failed")
        });
    }
    Ok(writer.length)
}

fn string_content_len(value: &str) -> Result<usize, ProviderCoreError> {
    encoded_len(value)?
        .checked_sub(2)
        .ok_or_else(|| error::malformed("reasoning string encoding was invalid"))
}

fn member_len(name: &str, value_len: usize) -> Result<usize, ProviderCoreError> {
    checked_sum(&[encoded_len(name)?, 1, value_len])
}

fn member_addition(
    entries: usize,
    name: &str,
    value_len: usize,
) -> Result<usize, ProviderCoreError> {
    member_len(name, value_len)?
        .checked_add(usize::from(entries != 0))
        .ok_or_else(|| error::limit("reasoning state size overflowed"))
}

fn checked_sum(values: &[usize]) -> Result<usize, ProviderCoreError> {
    values.iter().try_fold(0_usize, |total, value| {
        total
            .checked_add(*value)
            .ok_or_else(|| error::limit("reasoning state size overflowed"))
    })
}

fn adjusted_len(
    current: usize,
    removed: usize,
    added: usize,
    maximum: usize,
) -> Result<usize, ProviderCoreError> {
    let retained = current
        .checked_sub(removed)
        .ok_or_else(|| error::malformed("reasoning state size accounting changed"))?;
    let next = retained
        .checked_add(added)
        .ok_or_else(|| error::limit("reasoning state size overflowed"))?;
    if next > maximum {
        return Err(error::limit("reasoning state exceeded its aggregate bound"));
    }
    Ok(next)
}

struct CountingWriter {
    length: usize,
    overflowed: bool,
}

impl io::Write for CountingWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let Some(next) = self.length.checked_add(buffer.len()) else {
            self.overflowed = true;
            return Err(io::Error::new(io::ErrorKind::FileTooLarge, "reasoning size overflowed"));
        };
        self.length = next;
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

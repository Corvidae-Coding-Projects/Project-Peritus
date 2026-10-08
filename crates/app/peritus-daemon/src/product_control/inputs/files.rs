//! Exact daemon-owned file sources, paged without placing their bodies in model prompts.

use super::{
    CapturedConversation, CapturedFileReaders, ControlError, ControlStore, Error,
    FileReaderSlot,
};
use peritus_product_runner::{
    ContextSource, ContextSourcePage, ContextSourceSlice, MAX_CONTEXT_SOURCE_PAGE,
    MAX_CONTEXT_SOURCE_SLICE_BYTES,
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

impl CapturedConversation {
    /// Reconciles readers against this exact capture, retaining only exact same-owner sources.
    pub(crate) fn file_readers(
        &self,
        previous: Option<&Arc<CapturedFileReaders>>,
    ) -> Arc<CapturedFileReaders> {
        if let Some(previous) = previous
            && previous.matches(self)
        {
            return Arc::clone(previous);
        }
        Arc::new(CapturedFileReaders::new(
            self,
            previous.map(Arc::as_ref),
        ))
    }

    /// Lists one bounded page of the exact file versions selected by this input capture.
    pub(crate) fn context_sources(
        &self,
        after: Option<u64>,
    ) -> Result<ContextSourcePage, Error> {
        let start = match after {
            None => 0,
            Some(0) => return Err(ControlError::InvalidInput.into()),
            Some(after) => usize::try_from(after).map_err(|_| ControlError::InvalidInput)?,
        };
        if start > self.file_sources.len() {
            return Err(ControlError::NotFound.into());
        }
        let sources = self
            .file_sources
            .iter()
            .enumerate()
            .skip(start)
            .take(MAX_CONTEXT_SOURCE_PAGE)
            .map(|(index, source)| descriptor(index, source))
            .collect::<Result<Vec<_>, _>>()?;
        let next = if start.saturating_add(sources.len()) < self.file_sources.len() {
            sources.last().map(ContextSource::ordinal)
        } else {
            None
        };
        ContextSourcePage::new(after, sources, next)
            .map_err(|_| Error::Corrupt("invalid durable file-source page"))
    }

    pub(crate) fn context_source(
        &self,
        source: u64,
    ) -> Result<(ContextSource, peritus_product_runner::control::FileVersion), Error> {
        let index = source.checked_sub(1).ok_or(ControlError::InvalidInput)?;
        let index = usize::try_from(index).map_err(|_| ControlError::InvalidInput)?;
        let selected = self.file_sources.get(index).ok_or(ControlError::NotFound)?;
        Ok((descriptor(index, selected)?, selected.version.clone()))
    }
}

impl CapturedFileReaders {
    fn new(captured: &CapturedConversation, previous: Option<&Self>) -> Self {
        let previous = previous.filter(|previous| previous.same_owner(captured));
        let mut previous_by_operation = BTreeMap::new();
        if let Some(previous) = previous {
            for (index, source) in previous.file_sources.iter().enumerate() {
                previous_by_operation
                    .entry(source.version.operation())
                    .or_default()
                    .push(index);
            }
        }
        let readers = captured
            .file_sources
            .iter()
            .map(|source| {
                previous
                    .and_then(|previous| {
                        previous_by_operation
                            .get(&source.version.operation())
                            .and_then(|indices| {
                                indices.iter().find_map(|index| {
                                    if previous.file_sources.get(*index) == Some(source) {
                                        previous.readers.get(*index)
                                    } else {
                                        None
                                    }
                                })
                            })
                    })
                    .map_or_else(|| Arc::new(Mutex::new(None)), Arc::clone)
            })
            .collect();
        Self {
            conversation: captured.conversation,
            actor: captured.actor,
            workspace: captured.workspace,
            file_sources: captured.file_sources.clone(),
            readers,
        }
    }

    fn same_owner(&self, captured: &CapturedConversation) -> bool {
        self.conversation == captured.conversation
            && self.actor == captured.actor
            && self.workspace == captured.workspace
    }

    fn matches(&self, captured: &CapturedConversation) -> bool {
        self.same_owner(captured) && self.file_sources == captured.file_sources
    }

    fn reader(
        &self,
        captured: &CapturedConversation,
        source: u64,
    ) -> Result<FileReaderSlot, Error> {
        if !self.matches(captured) {
            return Err(ControlError::ScopeMismatch.into());
        }
        let index = source.checked_sub(1).ok_or(ControlError::InvalidInput)?;
        let index = usize::try_from(index).map_err(|_| ControlError::InvalidInput)?;
        self.readers
            .get(index)
            .cloned()
            .ok_or_else(|| ControlError::NotFound.into())
    }
}

impl ControlStore {
    /// Reads one exact UTF-8 page after revalidating capture scope and retained artifact bytes.
    pub(in crate::product_control) fn read_context_source(
        &self,
        captured: &CapturedConversation,
        readers: &CapturedFileReaders,
        source: u64,
        offset: u64,
    ) -> Result<ContextSourceSlice, Error> {
        let index = source.checked_sub(1).ok_or(ControlError::InvalidInput)?;
        let index = usize::try_from(index).map_err(|_| ControlError::InvalidInput)?;
        let selected =
            captured.file_sources.get(index).ok_or(ControlError::NotFound)?;
        let descriptor = descriptor(index, selected)?;
        let reader = readers.reader(captured, source)?;
        let (text, next) = self.file_text_slice(
            &selected.version,
            reader.as_ref(),
            offset,
            MAX_CONTEXT_SOURCE_SLICE_BYTES,
        )?;
        ContextSourceSlice::new(&descriptor, offset, text, next)
            .map_err(|_| Error::Corrupt("invalid durable file-source slice"))
    }
}

fn descriptor(
    index: usize,
    source: &super::manifest::FileSource,
) -> Result<ContextSource, Error> {
    let ordinal = u64::try_from(index)
        .ok()
        .and_then(|index| index.checked_add(1))
        .ok_or(ControlError::Capacity)?;
    let observation = source.version.observation();
    ContextSource::new(
        ordinal,
        source
            .attachment
            .source()
            .label()
            .chars()
            .flat_map(char::escape_debug)
            .collect(),
        observation.digest().into_bytes(),
        observation.bytes(),
    )
    .map_err(|_| Error::Corrupt("invalid durable file-source descriptor"))
}

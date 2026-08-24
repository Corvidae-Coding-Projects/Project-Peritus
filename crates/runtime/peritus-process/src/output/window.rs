//! Bounded in-memory tail window.

use std::collections::VecDeque;

pub(crate) struct RetainedWindow {
    bytes: VecDeque<u8>,
    limit: usize,
}

impl RetainedWindow {
    pub(crate) fn new(limit: u64) -> Self {
        Self { bytes: VecDeque::new(), limit: usize::try_from(limit).unwrap_or(usize::MAX) }
    }

    pub(crate) fn push(&mut self, chunk: &[u8]) {
        if chunk.len() >= self.limit {
            self.bytes.clear();
            self.bytes.extend(chunk[chunk.len() - self.limit..].iter().copied());
            return;
        }
        let overflow = self.bytes.len().saturating_add(chunk.len()).saturating_sub(self.limit);
        self.bytes.drain(..overflow);
        self.bytes.extend(chunk.iter().copied());
    }

    pub(crate) fn bytes(&self) -> Vec<u8> {
        self.bytes.iter().copied().collect()
    }
}

//! Shared immutable source pages: continuing a lineage copies only its last small page.

use super::{ObservationSource, WorkingError};
use std::sync::Arc;

pub(super) const PAGE_RECORDS: usize = 512;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct ObservationPrefix {
    pages: Vec<Arc<Vec<ObservationSource>>>,
    length: usize,
}

impl ObservationPrefix {
    pub(super) const fn len(&self) -> usize {
        self.length
    }

    pub(super) fn get(&self, index: usize) -> Option<&ObservationSource> {
        self.pages.get(index / PAGE_RECORDS)?.get(index % PAGE_RECORDS)
    }

    pub(super) fn push(&mut self, source: ObservationSource) -> Result<(), WorkingError> {
        if self.pages.last().is_none_or(|page| page.len() == PAGE_RECORDS) {
            self.pages.try_reserve(1).map_err(|_| WorkingError::Capacity)?;
            self.pages.push(Arc::new(Vec::new()));
        }
        let page = Arc::make_mut(self.pages.last_mut().ok_or(WorkingError::Capacity)?);
        page.try_reserve(1).map_err(|_| WorkingError::Capacity)?;
        page.push(source);
        self.length = self.length.checked_add(1).ok_or(WorkingError::Capacity)?;
        Ok(())
    }

    pub(super) const fn iter(&self) -> ObservationIter<'_> {
        ObservationIter { prefix: self, index: 0 }
    }
}

pub(super) struct ObservationIter<'a> {
    prefix: &'a ObservationPrefix,
    index: usize,
}
impl<'a> Iterator for ObservationIter<'a> {
    type Item = &'a ObservationSource;
    fn next(&mut self) -> Option<Self::Item> {
        let source = self.prefix.get(self.index)?;
        self.index += 1;
        Some(source)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        let left = self.prefix.len() - self.index;
        (left, Some(left))
    }
}
impl ExactSizeIterator for ObservationIter<'_> {}
impl<'a> IntoIterator for &'a ObservationPrefix {
    type Item = &'a ObservationSource;
    type IntoIter = ObservationIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

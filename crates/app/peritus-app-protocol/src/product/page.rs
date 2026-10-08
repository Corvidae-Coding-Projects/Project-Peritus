//! Snapshot-bound keyset pages for durable product-run history.

use peritus_types::{IdentifierError, RunId};

use super::{MAX_PRODUCT_RUN_PAGE, ProductRunMessageError, ProductRunObservation};

/// Stable identity of the durable daemon store that owns a run catalog.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProductRunStoreId([u8; 16]);

impl ProductRunStoreId {
    /// Creates a nonzero durable store identity.
    ///
    /// # Errors
    ///
    /// Rejects the reserved all-zero identity.
    pub const fn new(bytes: [u8; 16]) -> Result<Self, IdentifierError> {
        if bytes[0] != 0
            || bytes[1] != 0
            || bytes[2] != 0
            || bytes[3] != 0
            || bytes[4] != 0
            || bytes[5] != 0
            || bytes[6] != 0
            || bytes[7] != 0
            || bytes[8] != 0
            || bytes[9] != 0
            || bytes[10] != 0
            || bytes[11] != 0
            || bytes[12] != 0
            || bytes[13] != 0
            || bytes[14] != 0
            || bytes[15] != 0
        {
            Ok(Self(bytes))
        } else {
            Err(IdentifierError::Zero)
        }
    }

    /// Borrows the exact store identity bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

/// Immutable catalog boundary and last emitted key for a subsequent page.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductRunPageCursor {
    store: ProductRunStoreId,
    highwater_sequence: u64,
    highwater_run: RunId,
    after_sequence: u64,
    after_run: RunId,
}

impl ProductRunPageCursor {
    /// Creates a cursor whose last key is within its immutable high-water boundary.
    ///
    /// # Errors
    ///
    /// Rejects zero sequences or an after key outside the high-water boundary.
    pub fn new(
        store: ProductRunStoreId,
        highwater_sequence: u64,
        highwater_run: RunId,
        after_sequence: u64,
        after_run: RunId,
    ) -> Result<Self, ProductRunMessageError> {
        if highwater_sequence == 0
            || after_sequence == 0
            || after_sequence > highwater_sequence
            || after_sequence == highwater_sequence && after_run != highwater_run
        {
            return Err(ProductRunMessageError::InvalidPage);
        }
        Ok(Self {
            store,
            highwater_sequence,
            highwater_run,
            after_sequence,
            after_run,
        })
    }

    /// Returns the durable store that owns this cursor.
    #[must_use]
    pub const fn store(self) -> ProductRunStoreId {
        self.store
    }

    /// Returns the immutable first-page admission-sequence boundary.
    #[must_use]
    pub const fn highwater_sequence(self) -> u64 {
        self.highwater_sequence
    }

    /// Returns the immutable first-page run boundary.
    #[must_use]
    pub const fn highwater_run(self) -> RunId {
        self.highwater_run
    }

    /// Returns the admission sequence of the last emitted key.
    #[must_use]
    pub const fn after_sequence(self) -> u64 {
        self.after_sequence
    }

    /// Returns the run identity of the last emitted key.
    #[must_use]
    pub const fn after_run(self) -> RunId {
        self.after_run
    }
}

/// One durably ordered product-run observation in a catalog page.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunPageEntry {
    sequence: u64,
    observation: ProductRunObservation,
}

impl ProductRunPageEntry {
    /// Binds an observation to its immutable admission sequence.
    ///
    /// # Errors
    ///
    /// Rejects the reserved zero sequence.
    pub fn new(
        sequence: u64,
        observation: ProductRunObservation,
    ) -> Result<Self, ProductRunMessageError> {
        if sequence == 0 {
            return Err(ProductRunMessageError::InvalidPage);
        }
        Ok(Self { sequence, observation })
    }

    /// Returns the immutable admission sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Borrows the run observation.
    #[must_use]
    pub const fn observation(&self) -> &ProductRunObservation {
        &self.observation
    }
}

/// First-page or continuation query for a stable product-run catalog snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ProductRunPageQuery {
    cursor: Option<ProductRunPageCursor>,
}

impl ProductRunPageQuery {
    /// Starts a new immutable catalog snapshot.
    #[must_use]
    pub const fn first() -> Self {
        Self { cursor: None }
    }

    /// Continues the exact snapshot represented by `cursor`.
    #[must_use]
    pub const fn after(cursor: ProductRunPageCursor) -> Self {
        Self { cursor: Some(cursor) }
    }

    /// Returns the continuation cursor, if this is not a first-page query.
    #[must_use]
    pub const fn cursor(self) -> Option<ProductRunPageCursor> {
        self.cursor
    }
}

/// One bounded product-run catalog page with its durable snapshot continuation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProductRunPage {
    store: ProductRunStoreId,
    entries: Vec<ProductRunPageEntry>,
    next: Option<ProductRunPageCursor>,
}

impl ProductRunPage {
    /// Creates a bounded page whose continuation belongs to the same store.
    ///
    /// # Errors
    ///
    /// Rejects oversized, duplicate, unordered, or cursor-inconsistent pages.
    pub fn new(
        store: ProductRunStoreId,
        entries: Vec<ProductRunPageEntry>,
        next: Option<ProductRunPageCursor>,
    ) -> Result<Self, ProductRunMessageError> {
        let ordered = entries.windows(2).all(|pair| pair[0].sequence() > pair[1].sequence());
        let unique = entries.iter().enumerate().all(|(index, entry)| {
            entries[..index].iter().all(|prior| {
                prior.observation().snapshot().run_id()
                    != entry.observation().snapshot().run_id()
            })
        });
        let continuation = next.is_none_or(|cursor| {
            let Some(last) = entries.last() else { return false };
            cursor.store() == store
                && entries.len() == MAX_PRODUCT_RUN_PAGE
                && (cursor.after_sequence(), cursor.after_run())
                    == (last.sequence(), last.observation().snapshot().run_id())
                && entries.first().is_some_and(|first| {
                    first.sequence() < cursor.highwater_sequence()
                        || first.sequence() == cursor.highwater_sequence()
                            && first.observation().snapshot().run_id() == cursor.highwater_run()
                })
        });
        if entries.len() > MAX_PRODUCT_RUN_PAGE
            || !ordered
            || !unique
            || !continuation
        {
            return Err(ProductRunMessageError::InvalidPage);
        }
        Ok(Self { store, entries, next })
    }

    /// Returns the durable store that owns this catalog page.
    #[must_use]
    pub const fn store(&self) -> ProductRunStoreId {
        self.store
    }

    /// Borrows the bounded, durably ordered run entries.
    #[must_use]
    pub const fn entries(&self) -> &[ProductRunPageEntry] {
        self.entries.as_slice()
    }

    /// Returns the continuation for the same immutable snapshot, if more rows exist.
    #[must_use]
    pub const fn next(&self) -> Option<ProductRunPageCursor> {
        self.next
    }
}

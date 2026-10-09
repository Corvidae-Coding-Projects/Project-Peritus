//! Bounded canonical encoding helpers.

use crate::{MacosError, MacosOperation, error};

const MAX_CANONICAL_BYTES: usize = 512 * 1_024;
const MAX_COLLECTION_ITEMS: usize = 4_096;
const MAX_STRING_BYTES: usize = 256 * 1_024;

pub(crate) struct Writer {
    bytes: Vec<u8>,
}

impl Writer {
    pub(crate) const fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub(crate) fn u8(&mut self, value: u8) -> Result<(), MacosError> {
        self.fixed(&[value])
    }

    pub(crate) fn u16(&mut self, value: u16) -> Result<(), MacosError> {
        self.fixed(&value.to_be_bytes())
    }

    pub(crate) fn u32(&mut self, value: u32) -> Result<(), MacosError> {
        self.fixed(&value.to_be_bytes())
    }

    pub(crate) fn u64(&mut self, value: u64) -> Result<(), MacosError> {
        self.fixed(&value.to_be_bytes())
    }

    pub(crate) fn boolean(&mut self, value: bool) -> Result<(), MacosError> {
        self.u8(u8::from(value))
    }

    pub(crate) fn fixed(&mut self, value: &[u8]) -> Result<(), MacosError> {
        self.reserve(value.len())?;
        self.bytes.extend_from_slice(value);
        Ok(())
    }

    pub(crate) fn bytes(&mut self, value: &[u8]) -> Result<(), MacosError> {
        let length = u32::try_from(value.len())
            .map_err(|_| error::limited(MacosOperation::Manifest, "byte value is too large"))?;
        self.u32(length)?;
        self.fixed(value)
    }

    pub(crate) fn string(&mut self, value: &str) -> Result<(), MacosError> {
        if value.len() > MAX_STRING_BYTES {
            return Err(error::limited(MacosOperation::Manifest, "string exceeds manifest bound"));
        }
        self.bytes(value.as_bytes())
    }

    pub(crate) fn count(&mut self, value: usize) -> Result<(), MacosError> {
        if value > MAX_COLLECTION_ITEMS {
            return Err(error::limited(
                MacosOperation::Manifest,
                "collection exceeds manifest bound",
            ));
        }
        self.u32(
            u32::try_from(value)
                .map_err(|_| error::limited(MacosOperation::Manifest, "collection is too large"))?,
        )
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn reserve(&mut self, additional: usize) -> Result<(), MacosError> {
        let next =
            self.bytes.len().checked_add(additional).ok_or_else(|| {
                error::limited(MacosOperation::Manifest, "manifest size overflow")
            })?;
        if next > MAX_CANONICAL_BYTES {
            return Err(error::limited(MacosOperation::Manifest, "manifest exceeds byte bound"));
        }
        self.bytes.reserve(additional);
        Ok(())
    }
}

pub(crate) struct Reader<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(input: &'a [u8]) -> Result<Self, MacosError> {
        if input.len() > MAX_CANONICAL_BYTES {
            return Err(error::limited(MacosOperation::Manifest, "manifest exceeds byte bound"));
        }
        Ok(Self { input, offset: 0 })
    }

    pub(crate) fn u8(&mut self) -> Result<u8, MacosError> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u16(&mut self) -> Result<u16, MacosError> {
        Ok(u16::from_be_bytes(self.fixed()?))
    }

    pub(crate) fn u32(&mut self) -> Result<u32, MacosError> {
        Ok(u32::from_be_bytes(self.fixed()?))
    }

    pub(crate) fn u64(&mut self) -> Result<u64, MacosError> {
        Ok(u64::from_be_bytes(self.fixed()?))
    }

    pub(crate) fn boolean(&mut self) -> Result<bool, MacosError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(error::invalid(MacosOperation::Manifest, "invalid boolean tag")),
        }
    }

    pub(crate) fn fixed<const N: usize>(&mut self) -> Result<[u8; N], MacosError> {
        self.take(N)?
            .try_into()
            .map_err(|_| error::invalid(MacosOperation::Manifest, "invalid fixed-width value"))
    }

    pub(crate) fn bytes(&mut self) -> Result<&'a [u8], MacosError> {
        let length = usize::try_from(self.u32()?)
            .map_err(|_| error::limited(MacosOperation::Manifest, "byte length is too large"))?;
        if length > MAX_STRING_BYTES {
            return Err(error::limited(MacosOperation::Manifest, "byte value exceeds bound"));
        }
        self.take(length)
    }

    pub(crate) fn string(&mut self) -> Result<String, MacosError> {
        let value = self.bytes()?;
        String::from_utf8(value.to_vec())
            .map_err(|_| error::invalid(MacosOperation::Manifest, "manifest string is not UTF-8"))
    }

    pub(crate) fn count(&mut self) -> Result<usize, MacosError> {
        let value = usize::try_from(self.u32()?).map_err(|_| {
            error::limited(MacosOperation::Manifest, "collection count is too large")
        })?;
        if value > MAX_COLLECTION_ITEMS {
            return Err(error::limited(
                MacosOperation::Manifest,
                "collection exceeds manifest bound",
            ));
        }
        Ok(value)
    }

    pub(crate) fn finish(self) -> Result<(), MacosError> {
        if self.offset == self.input.len() {
            Ok(())
        } else {
            Err(error::invalid(MacosOperation::Manifest, "manifest has trailing bytes"))
        }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], MacosError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| error::limited(MacosOperation::Manifest, "manifest offset overflow"))?;
        let value = self
            .input
            .get(self.offset..end)
            .ok_or_else(|| error::invalid(MacosOperation::Manifest, "manifest is truncated"))?;
        self.offset = end;
        Ok(value)
    }
}

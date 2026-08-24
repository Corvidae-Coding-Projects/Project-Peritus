//! Bounded reader for canonical terminal results.

use peritus_types::Sha256Digest;

use crate::ProcessError;

use super::corrupt;

pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub(super) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    pub(super) const fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }

    fn take<const N: usize>(&mut self) -> Result<[u8; N], ProcessError> {
        let end =
            self.offset.checked_add(N).ok_or_else(|| corrupt("terminal offset overflowed"))?;
        let source = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| corrupt("terminal result is truncated"))?;
        let mut value = [0_u8; N];
        value.copy_from_slice(source);
        self.offset = end;
        Ok(value)
    }

    pub(super) fn u8(&mut self) -> Result<u8, ProcessError> {
        Ok(self.take::<1>()?[0])
    }

    pub(super) fn u16(&mut self) -> Result<u16, ProcessError> {
        Ok(u16::from_be_bytes(self.take()?))
    }

    pub(super) fn u32(&mut self) -> Result<u32, ProcessError> {
        Ok(u32::from_be_bytes(self.take()?))
    }

    pub(super) fn i32(&mut self) -> Result<i32, ProcessError> {
        Ok(i32::from_be_bytes(self.take()?))
    }

    pub(super) fn u64(&mut self) -> Result<u64, ProcessError> {
        Ok(u64::from_be_bytes(self.take()?))
    }

    pub(super) fn boolean(&mut self) -> Result<bool, ProcessError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(corrupt("terminal result has an invalid boolean")),
        }
    }

    pub(super) fn digest(&mut self) -> Result<Sha256Digest, ProcessError> {
        Ok(Sha256Digest::new(self.take()?))
    }

    pub(super) fn optional_u64(&mut self) -> Result<Option<u64>, ProcessError> {
        match self.u8()? {
            0 => Ok(None),
            1 => self.u64().map(Some),
            _ => Err(corrupt("terminal result has an invalid optional integer")),
        }
    }

    pub(super) fn count(&mut self, limit: usize) -> Result<usize, ProcessError> {
        let value = usize::from(self.u16()?);
        if value > limit {
            return Err(corrupt("terminal collection exceeds its canonical bound"));
        }
        Ok(value)
    }

    pub(super) fn string(&mut self, limit: usize) -> Result<String, ProcessError> {
        let length = self.count(limit)?;
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| corrupt("terminal string offset overflowed"))?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or_else(|| corrupt("terminal string is truncated"))?;
        self.offset = end;
        String::from_utf8(value.to_vec()).map_err(|_| corrupt("terminal string is not UTF-8"))
    }

    pub(super) fn id<T, E>(
        &mut self,
        constructor: impl FnOnce([u8; 16]) -> Result<T, E>,
    ) -> Result<T, ProcessError> {
        constructor(self.take()?).map_err(|_| corrupt("terminal result contains a zero identifier"))
    }
}

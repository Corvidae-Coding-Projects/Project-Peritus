//! Exact bounded readers for the B1 canonical field-record grammar.

use crate::ApprovalError;

const RECORD_MAGIC: &[u8] = b"PERITUS\0B1\0APPROVAL\0V1\0";

pub(super) const fn invalid() -> ApprovalError {
    ApprovalError::InvalidCanonicalEncoding
}

pub(super) struct CanonicalReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> CanonicalReader<'a> {
    pub(super) fn record(
        bytes: &'a [u8],
        domain: &[u8],
        maximum: usize,
    ) -> Result<Self, ApprovalError> {
        if bytes.len() > maximum {
            return Err(invalid());
        }
        let mut reader = Self { bytes, offset: 0 };
        if reader.take(RECORD_MAGIC.len())? != RECORD_MAGIC {
            return Err(invalid());
        }
        let domain_length = usize::from(reader.read_u16()?);
        if reader.take(domain_length)? != domain {
            return Err(invalid());
        }
        Ok(reader)
    }

    pub(super) fn field(&mut self, expected_tag: u16) -> Result<&'a [u8], ApprovalError> {
        if self.read_u16()? != expected_tag {
            return Err(invalid());
        }
        let length = usize::try_from(self.read_u64()?).map_err(|_| invalid())?;
        self.take(length)
    }

    pub(super) const fn finish(self) -> Result<(), ApprovalError> {
        if self.offset == self.bytes.len() { Ok(()) } else { Err(invalid()) }
    }

    fn read_u16(&mut self) -> Result<u16, ApprovalError> {
        Ok(u16::from_be_bytes(self.fixed()?))
    }

    fn read_u64(&mut self) -> Result<u64, ApprovalError> {
        Ok(u64::from_be_bytes(self.fixed()?))
    }

    fn fixed<const N: usize>(&mut self) -> Result<[u8; N], ApprovalError> {
        let mut value = [0_u8; N];
        value.copy_from_slice(self.take(N)?);
        Ok(value)
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ApprovalError> {
        let end = self.offset.checked_add(length).ok_or_else(invalid)?;
        let value = self.bytes.get(self.offset..end).ok_or_else(invalid)?;
        self.offset = end;
        Ok(value)
    }
}

pub(super) struct ListReader<'a> {
    bytes: &'a [u8],
    offset: usize,
    remaining: usize,
}

impl<'a> ListReader<'a> {
    pub(super) fn new(bytes: &'a [u8], maximum: usize) -> Result<Self, ApprovalError> {
        let mut reader = Self { bytes, offset: 0, remaining: 0 };
        let count = usize::try_from(reader.read_u32()?).map_err(|_| invalid())?;
        if count > maximum {
            return Err(invalid());
        }
        reader.remaining = count;
        Ok(reader)
    }

    pub(super) const fn len(&self) -> usize {
        self.remaining
    }

    pub(super) fn item(&mut self) -> Result<&'a [u8], ApprovalError> {
        if self.remaining == 0 {
            return Err(invalid());
        }
        let length = usize::try_from(self.read_u32()?).map_err(|_| invalid())?;
        let value = self.take(length)?;
        self.remaining -= 1;
        Ok(value)
    }

    pub(super) const fn finish(self) -> Result<(), ApprovalError> {
        if self.remaining == 0 && self.offset == self.bytes.len() { Ok(()) } else { Err(invalid()) }
    }

    fn read_u32(&mut self) -> Result<u32, ApprovalError> {
        let mut value = [0_u8; 4];
        value.copy_from_slice(self.take(4)?);
        Ok(u32::from_be_bytes(value))
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], ApprovalError> {
        let end = self.offset.checked_add(length).ok_or_else(invalid)?;
        let value = self.bytes.get(self.offset..end).ok_or_else(invalid)?;
        self.offset = end;
        Ok(value)
    }
}

pub(super) fn encode_list<T>(
    values: &[T],
    mut encode: impl FnMut(&T) -> Result<Vec<u8>, ApprovalError>,
) -> Result<Vec<u8>, ApprovalError> {
    let count = u32::try_from(values.len()).map_err(|_| ApprovalError::PreimageTooLarge)?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&count.to_be_bytes());
    for value in values {
        let item = encode(value)?;
        let length = u32::try_from(item.len()).map_err(|_| ApprovalError::PreimageTooLarge)?;
        bytes.extend_from_slice(&length.to_be_bytes());
        bytes.extend_from_slice(&item);
    }
    Ok(bytes)
}

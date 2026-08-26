//! Length-prefixed, domain-separated canonical encoding helpers.

use peritus_types::{HarnessId, RevisionNumber, Sha256Digest};

use crate::domain::{HarnessDomainError, HarnessDomainErrorKind};

pub struct CanonicalEncoder {
    bytes: Vec<u8>,
}

impl CanonicalEncoder {
    pub fn new(domain: &[u8]) -> Self {
        let mut encoder = Self { bytes: Vec::with_capacity(256) };
        encoder.bytes(domain);
        encoder
    }

    pub fn u8(&mut self, value: u8) {
        self.bytes.push(value);
    }
    pub fn u16(&mut self, value: u16) {
        self.raw(&value.to_be_bytes());
    }
    pub fn u32(&mut self, value: u32) {
        self.raw(&value.to_be_bytes());
    }
    pub fn u64(&mut self, value: u64) {
        self.raw(&value.to_be_bytes());
    }
    pub fn bool(&mut self, value: bool) {
        self.u8(u8::from(value));
    }
    pub fn raw(&mut self, value: &[u8]) {
        self.bytes.extend_from_slice(value);
    }

    pub fn len(&mut self, value: usize) {
        self.u64(u64::try_from(value).unwrap_or(u64::MAX));
    }

    pub fn bytes(&mut self, value: &[u8]) {
        self.len(value.len());
        self.raw(value);
    }

    pub fn string(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }
    pub fn digest(&mut self, value: Sha256Digest) {
        self.raw(value.as_bytes());
    }
    pub fn harness_id(&mut self, value: HarnessId) {
        self.raw(value.as_bytes());
    }
    pub fn revision_number(&mut self, value: RevisionNumber) {
        self.u64(value.get());
    }

    pub fn optional_digest(&mut self, value: Option<Sha256Digest>) {
        self.bool(value.is_some());
        if let Some(value) = value {
            self.digest(value);
        }
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }
}

pub struct CanonicalReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> CanonicalReader<'a> {
    pub fn new(bytes: &'a [u8], domain: &[u8]) -> Result<Self, HarnessDomainError> {
        let mut reader = Self { bytes, offset: 0 };
        if reader.byte_slice()? != domain {
            return Err(invalid_encoding("canonical domain separator mismatch"));
        }
        Ok(reader)
    }

    pub fn u8(&mut self) -> Result<u8, HarnessDomainError> {
        Ok(self.take(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16, HarnessDomainError> {
        let bytes: [u8; 2] = self.take(2)?.try_into().map_err(|_| invalid_encoding("u16"))?;
        Ok(u16::from_be_bytes(bytes))
    }

    pub fn u32(&mut self) -> Result<u32, HarnessDomainError> {
        let bytes: [u8; 4] = self.take(4)?.try_into().map_err(|_| invalid_encoding("u32"))?;
        Ok(u32::from_be_bytes(bytes))
    }

    pub fn u64(&mut self) -> Result<u64, HarnessDomainError> {
        let bytes: [u8; 8] = self.take(8)?.try_into().map_err(|_| invalid_encoding("u64"))?;
        Ok(u64::from_be_bytes(bytes))
    }

    pub fn bool(&mut self) -> Result<bool, HarnessDomainError> {
        match self.u8()? {
            0 => Ok(false),
            1 => Ok(true),
            _ => Err(invalid_encoding("noncanonical boolean tag")),
        }
    }

    pub fn length(&mut self) -> Result<usize, HarnessDomainError> {
        usize::try_from(self.u64()?).map_err(|_| invalid_encoding("length exceeds platform size"))
    }

    pub fn byte_slice(&mut self) -> Result<&'a [u8], HarnessDomainError> {
        let length = self.length()?;
        self.take(length)
    }

    pub fn string(&mut self) -> Result<String, HarnessDomainError> {
        let bytes = self.byte_slice()?;
        let value = std::str::from_utf8(bytes)
            .map_err(|_| invalid_encoding("canonical string is not UTF-8"))?;
        Ok(value.to_owned())
    }

    pub fn digest(&mut self) -> Result<Sha256Digest, HarnessDomainError> {
        let bytes: [u8; 32] = self.take(32)?.try_into().map_err(|_| invalid_encoding("digest"))?;
        Ok(Sha256Digest::new(bytes))
    }

    pub fn harness_id(&mut self) -> Result<HarnessId, HarnessDomainError> {
        let bytes: [u8; 16] =
            self.take(16)?.try_into().map_err(|_| invalid_encoding("harness ID"))?;
        HarnessId::new(bytes).map_err(|_| invalid_encoding("reserved zero harness ID"))
    }

    pub fn revision_number(&mut self) -> Result<RevisionNumber, HarnessDomainError> {
        RevisionNumber::new(self.u64()?).map_err(|_| invalid_encoding("invalid revision number"))
    }

    pub fn optional_digest(&mut self) -> Result<Option<Sha256Digest>, HarnessDomainError> {
        if self.bool()? { Ok(Some(self.digest()?)) } else { Ok(None) }
    }

    pub fn finish(self) -> Result<(), HarnessDomainError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(invalid_encoding("trailing canonical bytes"))
        }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], HarnessDomainError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| invalid_encoding("canonical offset overflow"))?;
        if end > self.bytes.len() {
            return Err(invalid_encoding("truncated canonical value"));
        }
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }
}

pub fn invalid_encoding(detail: &'static str) -> HarnessDomainError {
    HarnessDomainError::detail(HarnessDomainErrorKind::InvalidCanonicalEncoding, detail)
}

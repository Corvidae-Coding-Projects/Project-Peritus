//! Small canonical encoding helpers shared by manifests and recovery records.

use crate::{LinuxError, LinuxErrorKind, LinuxOperation, LinuxRecovery};

pub const MAX_PROTOCOL_BYTES: usize = 1024 * 1024;
pub const MAX_ITEMS: usize = 256;

pub fn push_bytes(output: &mut Vec<u8>, value: &[u8]) -> Result<(), LinuxError> {
    let length = u32::try_from(value.len()).map_err(|_| protocol_error("value is too large"))?;
    output.extend_from_slice(&length.to_be_bytes());
    output.extend_from_slice(value);
    check_total(output)
}

pub fn push_str(output: &mut Vec<u8>, value: &str) -> Result<(), LinuxError> {
    push_bytes(output, value.as_bytes())
}

pub fn push_count(output: &mut Vec<u8>, count: usize) -> Result<(), LinuxError> {
    if count > MAX_ITEMS {
        return Err(protocol_error("collection exceeds protocol bound"));
    }
    let count = u32::try_from(count).map_err(|_| protocol_error("collection is too large"))?;
    output.extend_from_slice(&count.to_be_bytes());
    check_total(output)
}

pub fn check_total(output: &[u8]) -> Result<(), LinuxError> {
    if output.len() > MAX_PROTOCOL_BYTES {
        Err(protocol_error("protocol payload exceeds one mebibyte"))
    } else {
        Ok(())
    }
}

pub fn protocol_error(detail: &'static str) -> LinuxError {
    LinuxError::new(
        LinuxErrorKind::Helper,
        LinuxOperation::Manifest,
        LinuxRecovery::CorrectRequest,
        detail,
    )
}

#[derive(Debug)]
pub struct Reader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub(crate) const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }
    pub(crate) fn u8(&mut self) -> Result<u8, LinuxError> {
        Ok(self.take(1)?[0])
    }
    pub(crate) fn u16(&mut self) -> Result<u16, LinuxError> {
        Ok(u16::from_be_bytes(self.take(2)?.try_into().expect("exact checked length")))
    }
    pub(crate) fn u32(&mut self) -> Result<u32, LinuxError> {
        Ok(u32::from_be_bytes(self.take(4)?.try_into().expect("exact checked length")))
    }
    pub(crate) fn u64(&mut self) -> Result<u64, LinuxError> {
        Ok(u64::from_be_bytes(self.take(8)?.try_into().expect("exact checked length")))
    }
    pub(crate) fn fixed<const N: usize>(&mut self) -> Result<[u8; N], LinuxError> {
        Ok(self.take(N)?.try_into().expect("exact checked length"))
    }
    pub(crate) fn bytes(&mut self) -> Result<Vec<u8>, LinuxError> {
        let length = usize::try_from(self.u32()?).map_err(|_| protocol_error("bad length"))?;
        if length > MAX_PROTOCOL_BYTES {
            return Err(protocol_error("field exceeds protocol bound"));
        }
        Ok(self.take(length)?.to_vec())
    }
    pub(crate) fn string(&mut self) -> Result<String, LinuxError> {
        String::from_utf8(self.bytes()?).map_err(|_| protocol_error("field is not UTF-8"))
    }
    pub(crate) fn count(&mut self) -> Result<usize, LinuxError> {
        let count = usize::try_from(self.u32()?).map_err(|_| protocol_error("bad count"))?;
        if count > MAX_ITEMS {
            Err(protocol_error("collection exceeds protocol bound"))
        } else {
            Ok(count)
        }
    }
    pub(crate) fn finish(self) -> Result<(), LinuxError> {
        if self.offset == self.bytes.len() {
            Ok(())
        } else {
            Err(protocol_error("trailing protocol bytes"))
        }
    }
    fn take(&mut self, count: usize) -> Result<&'a [u8], LinuxError> {
        let end = self.offset.checked_add(count).ok_or_else(|| protocol_error("bad length"))?;
        if end > self.bytes.len() {
            return Err(protocol_error("truncated protocol value"));
        }
        let value = &self.bytes[self.offset..end];
        self.offset = end;
        Ok(value)
    }
}

pub fn digest_hex(digest: peritus_types::Sha256Digest) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(64);
    for byte in digest.as_bytes() {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

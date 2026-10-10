//! Bounded canonical encoding helpers.

use crate::{MacosError, MacosOperation, error};

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
        self.bytes(value.as_bytes())
    }

    #[cfg(unix)]
    pub(crate) fn path(&mut self, value: &std::path::Path) -> Result<(), MacosError> {
        use std::os::unix::ffi::OsStrExt as _;
        self.bytes(value.as_os_str().as_bytes())
    }

    // The macOS wire format uses Unix path bytes. Other hosts may handle its UTF-8 subset
    // exactly, but must reject native names that have no exact representation in that format.
    #[cfg(not(unix))]
    pub(crate) fn path(&mut self, value: &std::path::Path) -> Result<(), MacosError> {
        self.string(value.to_str().ok_or_else(|| {
            error::invalid(
                MacosOperation::Manifest,
                "path cannot be represented as Unix bytes on this host",
            )
        })?)
    }

    pub(crate) fn count(&mut self, value: usize) -> Result<(), MacosError> {
        self.u32(
            u32::try_from(value)
                .map_err(|_| error::limited(MacosOperation::Manifest, "collection is too large"))?,
        )
    }

    pub(crate) fn finish(self) -> Vec<u8> {
        self.bytes
    }

    fn reserve(&mut self, additional: usize) -> Result<(), MacosError> {
        self.bytes
            .len()
            .checked_add(additional)
            .ok_or_else(|| error::limited(MacosOperation::Manifest, "manifest size overflow"))?;
        self.bytes.try_reserve(additional).map_err(|_| {
            error::limited(MacosOperation::Manifest, "manifest storage cannot be reserved")
        })?;
        Ok(())
    }
}

pub(crate) struct Reader<'a> {
    input: &'a [u8],
    offset: usize,
}

impl<'a> Reader<'a> {
    pub(crate) const fn new(input: &'a [u8]) -> Self {
        Self { input, offset: 0 }
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
        self.take(length)
    }

    #[cfg(unix)]
    pub(crate) fn path(&mut self) -> Result<std::path::PathBuf, MacosError> {
        use std::os::unix::ffi::OsStringExt as _;
        let value = self.bytes()?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(value.len()).map_err(|_| {
            error::limited(MacosOperation::Manifest, "manifest path storage cannot be reserved")
        })?;
        bytes.extend_from_slice(value);
        Ok(std::ffi::OsString::from_vec(bytes).into())
    }

    #[cfg(not(unix))]
    pub(crate) fn path(&mut self) -> Result<std::path::PathBuf, MacosError> {
        Ok(self.string()?.into())
    }

    pub(crate) fn string(&mut self) -> Result<String, MacosError> {
        let value = self.bytes()?;
        let mut text = String::new();
        text.try_reserve(value.len()).map_err(|_| {
            error::limited(MacosOperation::Manifest, "manifest string storage cannot be reserved")
        })?;
        text.push_str(std::str::from_utf8(value).map_err(|_| {
            error::invalid(MacosOperation::Manifest, "manifest string is not UTF-8")
        })?);
        Ok(text)
    }

    pub(crate) fn count(&mut self) -> Result<usize, MacosError> {
        let value = usize::try_from(self.u32()?).map_err(|_| {
            error::limited(MacosOperation::Manifest, "collection count is too large")
        })?;
        if value > self.input.len().saturating_sub(self.offset) {
            return Err(error::limited(
                MacosOperation::Manifest,
                "collection count exceeds the remaining manifest bytes",
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

#[cfg(test)]
mod tests {
    use super::{Reader, Writer};
    use std::path::PathBuf;

    #[test]
    fn canonical_fields_accept_wire_sized_strings_and_collections() {
        let long = "x".repeat(300 * 1024);
        let mut writer = Writer::new();
        writer.string(&long).expect("string beyond former field limit");
        writer.count(4_097).expect("count beyond former collection limit");
        writer.fixed(&vec![0; 4_097]).expect("collection body");
        let bytes = writer.finish();
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.string().expect("long string"), long);
        assert_eq!(reader.count().expect("large count"), 4_097);
    }

    #[test]
    fn portable_path_round_trip_preserves_wire_bytes() {
        let path = PathBuf::from("/workspace/project");
        let mut writer = Writer::new();
        writer.path(&path).expect("portable path");
        let bytes = writer.finish();
        assert_eq!(bytes, [18_u32.to_be_bytes().as_slice(), b"/workspace/project"].concat());
        let mut reader = Reader::new(&bytes);
        assert_eq!(reader.path().expect("portable decoded path"), path);
        reader.finish().expect("exact consumption");
    }

    #[cfg(windows)]
    #[test]
    fn non_unix_host_rejects_unrepresentable_paths_without_loss() {
        use std::os::windows::ffi::OsStringExt as _;
        let path = PathBuf::from(std::ffi::OsString::from_wide(&[0xd800]));
        assert!(Writer::new().path(&path).is_err());
        let bytes = [1_u32.to_be_bytes().as_slice(), &[0xff]].concat();
        assert!(Reader::new(&bytes).path().is_err());
    }

    #[cfg(unix)]
    #[test]
    fn native_path_round_trip_preserves_non_utf8_bytes_and_utf8_encoding() {
        use std::os::unix::ffi::{OsStrExt as _, OsStringExt as _};

        let path = PathBuf::from(std::ffi::OsString::from_vec(vec![b'/', 0xff]));
        let mut writer = Writer::new();
        writer.path(&path).expect("native path");
        let bytes = writer.finish();
        let mut reader = Reader::new(&bytes);
        let decoded = reader.path().expect("decoded native path");
        assert_eq!(decoded.as_os_str().as_bytes(), [b'/', 0xff]);

        let utf8 = PathBuf::from("/workspace/project");
        let mut writer = Writer::new();
        writer.path(&utf8).expect("portable UTF-8 path");
        assert_eq!(
            writer.finish(),
            [18_u32.to_be_bytes().as_slice(), b"/workspace/project"].concat()
        );
    }
}

//! Additive manifest forms preserve exact historical bytes and have no logical item allowance.

use peritus_codec::{CanonicalReader, CanonicalWriter, CodecError, CodecErrorKind};

#[derive(Clone, Copy)]
pub(super) enum ManifestForm {
    Legacy,
    Wide,
}

impl ManifestForm {
    pub(super) const fn for_wide(wide: bool) -> Self {
        if wide { Self::Wide } else { Self::Legacy }
    }
    pub(super) const fn is_wide(self) -> bool {
        matches!(self, Self::Wide)
    }
    pub(super) const fn check(self, wide: bool, offset: usize) -> Result<(), CodecError> {
        if self.is_wide() == wide {
            Ok(())
        } else {
            Err(CodecError::at(CodecErrorKind::InvalidDomainValue, offset))
        }
    }
    pub(super) fn write_count(
        self,
        w: &mut CanonicalWriter,
        count: usize,
    ) -> Result<(), CodecError> {
        match self {
            Self::Legacy => w.write_u16(
                u16::try_from(count)
                    .map_err(|_| CodecError::at(CodecErrorKind::LengthOverflow, w.len()))?,
            ),
            Self::Wide => w.write_u64(
                u64::try_from(count)
                    .map_err(|_| CodecError::at(CodecErrorKind::LengthOverflow, w.len()))?,
            ),
        }
    }
    pub(super) fn read_count(
        self,
        r: &mut CanonicalReader<'_>,
        minimum_item_bytes: usize,
    ) -> Result<usize, CodecError> {
        let offset = r.offset();
        let count = match self {
            Self::Legacy => usize::from(r.read_u16()?),
            Self::Wide => usize::try_from(r.read_u64()?)
                .map_err(|_| CodecError::at(CodecErrorKind::LengthOverflow, offset))?,
        };
        let minimum = count
            .checked_mul(minimum_item_bytes)
            .ok_or_else(|| CodecError::at(CodecErrorKind::LengthOverflow, offset))?;
        if minimum > r.remaining() {
            return Err(CodecError::at(CodecErrorKind::Truncated, r.offset()));
        }
        Ok(count)
    }
}

pub fn legacy_text(text: &str, width: usize) -> bool {
    text.len() <= width && !text.chars().any(char::is_control)
}

pub fn wide_lists<'a>(
    mut paths: impl ExactSizeIterator<Item = &'a str>,
    exclusions: &[String],
    external: &[String],
) -> bool {
    paths.len() > usize::from(u16::MAX)
        || paths.any(|path| !legacy_text(path, 4096))
        || [exclusions, external].into_iter().any(|values| {
            values.len() > usize::from(u16::MAX)
                || values.iter().any(|text| !legacy_text(text, 512))
        })
}

//! Streaming selection resolution shared by capture and partial-range restoration.

use super::{ControlError, Error};
use peritus_product_runner::control::FileRange;
use std::{collections::BTreeMap, io::Read};

/// Resolves every selection against one complete source scan without a source-size ceiling.
/// Fixed byte selections retain their offsets; line selections retain original terminators.
pub(super) fn resolve_ranges(
    input: &mut dyn Read,
    selections: &[FileRange],
    size: u64,
) -> Result<Vec<(u64, u64)>, Error> {
    let mut boundaries = BTreeMap::new();
    for selection in selections {
        match *selection {
            FileRange::Lines { first, last } if first > 0 && first <= last => {
                boundaries.insert(u64::from(first), None);
                boundaries.insert(u64::from(last) + 1, None);
            }
            FileRange::Bytes { start, end } if start < end && end <= size => {}
            FileRange::All => {}
            _ => return Err(ControlError::InvalidInput.into()),
        }
    }
    let mut last_seen = 0;
    if !boundaries.is_empty() {
        if let Some(start) = boundaries.get_mut(&1) {
            *start = Some(0);
        }
        let mut line = 1_u64;
        let mut offset = 0_u64;
        let mut chunk = vec![0_u8; 64 * 1024];
        loop {
            let count = input.read(&mut chunk)?;
            if count == 0 {
                break;
            }
            for byte in &chunk[..count] {
                offset = offset.checked_add(1).ok_or(ControlError::Capacity)?;
                last_seen = line;
                if *byte == b'\n' {
                    line = line.checked_add(1).ok_or(ControlError::Capacity)?;
                    if let Some(start) = boundaries.get_mut(&line) {
                        *start = Some(offset);
                    }
                }
            }
            if offset > size {
                return Err(Error::StalePreimage);
            }
        }
        if offset != size {
            return Err(Error::StalePreimage);
        }
    }
    selections
        .iter()
        .map(|selection| match *selection {
            FileRange::All => Ok((0, size)),
            FileRange::Bytes { start, end } => Ok((start, end)),
            FileRange::Lines { first, last } if u64::from(last) <= last_seen => {
                let start = boundaries
                    .get(&u64::from(first))
                    .copied()
                    .flatten()
                    .ok_or(ControlError::InvalidInput)?;
                let end = boundaries.get(&(u64::from(last) + 1)).copied().flatten().unwrap_or(size);
                if start >= end {
                    return Err(ControlError::InvalidInput.into());
                }
                Ok((start, end))
            }
            FileRange::Lines { .. } => Err(ControlError::InvalidInput.into()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selection_resolution_has_no_complete_source_ceiling_and_retains_an_unterminated_line() {
        let size = 64 * 1024 * 1024 + 1;
        let mut source = std::io::repeat(b'x').take(size);
        let selections = [
            FileRange::Lines { first: 1, last: 1 },
            FileRange::Bytes { start: size - 1, end: size },
        ];
        assert_eq!(
            resolve_ranges(&mut source, &selections, size).unwrap(),
            vec![(0, size), (size - 1, size)]
        );
    }
}

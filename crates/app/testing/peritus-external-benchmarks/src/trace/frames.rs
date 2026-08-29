//! Checked decoding for the product runner's length-framed developer trace.

use std::{
    fs::File,
    io::{self, BufReader, Read as _},
    path::Path,
};

use crate::BenchmarkError;

const MAX_FRAME_BYTES: u64 = 32 * 1024 * 1024;

pub(super) struct Frame {
    pub tag: u8,
    pub payload: Vec<u8>,
}

pub(super) fn read(path: &Path) -> Result<Vec<Frame>, BenchmarkError> {
    let file = File::open(path)
        .map_err(|error| BenchmarkError::filesystem("open developer trace", path, error))?;
    let mut reader = BufReader::new(file);
    let mut frames = Vec::new();
    loop {
        let mut tag = [0_u8; 1];
        match reader.read(&mut tag) {
            Ok(0) => break,
            Ok(1) => {}
            Ok(_) => unreachable!("one-byte read returned more than one byte"),
            Err(error) => return Err(BenchmarkError::filesystem("read trace tag", path, error)),
        }
        if !matches!(tag[0], 1..=5) {
            return Err(BenchmarkError::trace(path, "trace contains an unknown frame tag"));
        }
        let mut length = [0_u8; 8];
        read_exact(&mut reader, &mut length, path, "trace frame length")?;
        let length = u64::from_le_bytes(length);
        if length > MAX_FRAME_BYTES {
            return Err(BenchmarkError::trace(path, "trace frame exceeds its byte bound"));
        }
        let length = usize::try_from(length)
            .map_err(|_| BenchmarkError::trace(path, "trace frame length is not representable"))?;
        let mut payload = vec![0_u8; length];
        read_exact(&mut reader, &mut payload, path, "trace frame payload")?;
        frames.push(Frame { tag: tag[0], payload });
    }
    Ok(frames)
}

fn read_exact(
    reader: &mut BufReader<File>,
    bytes: &mut [u8],
    path: &Path,
    field: &'static str,
) -> Result<(), BenchmarkError> {
    reader.read_exact(bytes).map_err(|error| {
        let detail = if error.kind() == io::ErrorKind::UnexpectedEof {
            format!("{field} is truncated")
        } else {
            format!("{field} could not be read: {error}")
        };
        BenchmarkError::trace(path, detail)
    })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn rejects_unknown_and_truncated_frames() {
        let root = tempfile::tempdir().expect("temporary trace");
        let unknown = root.path().join("unknown.trace");
        fs::write(&unknown, [9_u8, 0, 0, 0, 0, 0, 0, 0, 0]).expect("unknown trace");
        assert!(read(&unknown).is_err());

        let truncated = root.path().join("truncated.trace");
        fs::write(&truncated, [1_u8, 3, 0, 0, 0, 0, 0, 0, 0, b'a']).expect("trace");
        assert!(read(&truncated).is_err());
    }
}

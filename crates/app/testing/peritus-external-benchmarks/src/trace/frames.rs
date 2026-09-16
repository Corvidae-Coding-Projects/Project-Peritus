//! Checked decoding for the product runner's length-framed developer trace.

use std::{
    fs::File,
    io::{self, BufReader, Read as _},
    path::Path,
};

use crate::BenchmarkError;
use peritus_product_runner::DeveloperTraceFrameKind;

const MAX_FRAME_BYTES: u64 = 32 * 1024 * 1024;

pub(super) struct Frame {
    pub kind: DeveloperTraceFrameKind,
    pub payload: Vec<u8>,
}

pub(super) fn read(path: &Path) -> Result<Vec<Frame>, BenchmarkError> {
    let file = File::open(path)
        .map_err(|error| BenchmarkError::trace(path, format!("open developer trace: {error}")))?;
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
        let kind = DeveloperTraceFrameKind::from_tag(tag[0])
            .ok_or_else(|| BenchmarkError::trace(path, "trace contains an unknown frame tag"))?;
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
        frames.push(Frame { kind, payload });
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

    #[test]
    fn accepts_every_product_trace_frame_kind() {
        let root = tempfile::tempdir().expect("temporary trace");
        let path = root.path().join("known.trace");
        let mut bytes = Vec::new();
        for tag in 1_u8..=7 {
            bytes.push(tag);
            bytes.extend_from_slice(&0_u64.to_le_bytes());
        }
        fs::write(&path, bytes).expect("known trace");

        let frames = read(&path).expect("known trace kinds");

        assert_eq!(frames.len(), 7);
        for (frame, tag) in frames.iter().zip(1_u8..=7) {
            assert_eq!(frame.kind.tag(), tag);
        }
    }
}

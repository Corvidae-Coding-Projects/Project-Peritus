//! Bounded in-memory tail window with stream-preserving reconstruction.

use std::collections::VecDeque;

use crate::OutputStream;

struct RetainedChunk {
    stream: OutputStream,
    bytes: VecDeque<u8>,
}

pub(crate) struct RetainedWindow {
    chunks: VecDeque<RetainedChunk>,
    length: usize,
    limit: usize,
}

impl RetainedWindow {
    pub(crate) fn new(limit: u64) -> Self {
        Self {
            chunks: VecDeque::new(),
            length: 0,
            limit: usize::try_from(limit).unwrap_or(usize::MAX),
        }
    }

    pub(crate) fn push(&mut self, stream: OutputStream, chunk: &[u8]) {
        if self.limit == 0 {
            return;
        }
        if chunk.len() >= self.limit {
            self.chunks.clear();
            self.chunks.push_back(RetainedChunk {
                stream,
                bytes: chunk[chunk.len() - self.limit..].iter().copied().collect(),
            });
            self.length = self.limit;
            return;
        }
        self.discard_front(self.length.saturating_add(chunk.len()).saturating_sub(self.limit));
        if let Some(last) = self.chunks.back_mut()
            && last.stream == stream
        {
            last.bytes.extend(chunk.iter().copied());
        } else {
            self.chunks.push_back(RetainedChunk { stream, bytes: chunk.iter().copied().collect() });
        }
        self.length = self.length.saturating_add(chunk.len());
    }

    pub(crate) fn stream_bytes(&self, stream: OutputStream) -> Vec<u8> {
        let mut bytes = Vec::new();
        for chunk in self.chunks.iter().filter(|chunk| chunk.stream == stream) {
            let (first, second) = chunk.bytes.as_slices();
            bytes.extend_from_slice(first);
            bytes.extend_from_slice(second);
        }
        bytes
    }

    fn discard_front(&mut self, mut count: usize) {
        while count > 0 {
            let Some(front) = self.chunks.front_mut() else { break };
            let discarded = count.min(front.bytes.len());
            front.bytes.drain(..discarded);
            self.length = self.length.saturating_sub(discarded);
            count -= discarded;
            if front.bytes.is_empty() {
                self.chunks.pop_front();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_stream_bytes_without_exceeding_the_combined_tail_limit() {
        let mut window = RetainedWindow::new(8);
        window.push(OutputStream::Stdout, b"abc");
        window.push(OutputStream::Stderr, b"12");
        window.push(OutputStream::Stdout, b"defg");
        assert_eq!(window.stream_bytes(OutputStream::Stdout), b"bcdefg");
        assert_eq!(window.stream_bytes(OutputStream::Stderr), b"12");
        assert_eq!(window.length, 8);
    }

    #[test]
    fn wrapped_and_interleaved_chunks_preserve_exact_stream_order_after_eviction() {
        let mut window = RetainedWindow::new(8);
        window.push(OutputStream::Stdout, b"abcdefgh");
        let mut expected = b"abcdefgh".to_vec();
        let capacity = window.chunks[0].bytes.capacity();
        for index in 0..capacity {
            let byte = b'A' + u8::try_from(index % 26).expect("ASCII letter offset");
            window.push(OutputStream::Stdout, &[byte]);
            expected.remove(0);
            expected.push(byte);
            assert_eq!(window.stream_bytes(OutputStream::Stdout), expected);
            if !window.chunks[0].bytes.as_slices().1.is_empty() {
                break;
            }
        }
        assert!(!window.chunks[0].bytes.as_slices().1.is_empty(), "exercise both ring slices");

        window.push(OutputStream::Stdout, b"abcdefghij");
        assert_eq!(window.stream_bytes(OutputStream::Stdout), b"cdefghij");

        window.push(OutputStream::Stderr, b"12");
        window.push(OutputStream::Terminal, b"XYZ");
        assert_eq!(window.stream_bytes(OutputStream::Stdout), b"hij");
        assert_eq!(window.stream_bytes(OutputStream::Stderr), b"12");
        assert_eq!(window.stream_bytes(OutputStream::Terminal), b"XYZ");

        window.push(OutputStream::Stdout, b"Zebra");
        assert_eq!(window.stream_bytes(OutputStream::Stdout), b"Zebra");
        assert!(window.stream_bytes(OutputStream::Stderr).is_empty());
        assert_eq!(window.stream_bytes(OutputStream::Terminal), b"XYZ");

        window.push(OutputStream::Stderr, b"0123456789");
        assert!(window.stream_bytes(OutputStream::Stdout).is_empty());
        assert_eq!(window.stream_bytes(OutputStream::Stderr), b"23456789");
        assert!(window.stream_bytes(OutputStream::Terminal).is_empty());
        assert_eq!(window.length, 8);
    }
}

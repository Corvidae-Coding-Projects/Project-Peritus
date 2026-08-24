//! Structural validation and deterministic request matching.

use super::model::{FakeHttpFault, FakeHttpHeader, FakeHttpReleasePoint, HeaderMatchMode};

pub fn headers_match(
    expected: &[FakeHttpHeader],
    actual: &[FakeHttpHeader],
    mode: HeaderMatchMode,
) -> bool {
    match mode {
        HeaderMatchMode::Exact => {
            expected.len() == actual.len() && expected.iter().zip(actual).all(header_equal)
        }
        HeaderMatchMode::AllowAdditional => ordered_subset(expected, actual),
    }
}

pub fn encoded_header_bytes(headers: &[FakeHttpHeader]) -> Option<usize> {
    headers.iter().try_fold(0_usize, |total, header| {
        total.checked_add(header.name().len())?.checked_add(header.value().len())?.checked_add(4)
    })
}

pub const fn valid_chunk_index(fault: FakeHttpFault, chunks: usize) -> bool {
    match fault {
        FakeHttpFault::Complete | FakeHttpFault::CloseAfterHeaders => true,
        FakeHttpFault::CloseAfterChunks(count) => count <= chunks,
    }
}

pub const fn valid_release(
    release: Option<FakeHttpReleasePoint>,
    fault: FakeHttpFault,
    chunks: usize,
) -> bool {
    match release {
        None | Some(FakeHttpReleasePoint::BeforeHeaders) => true,
        Some(FakeHttpReleasePoint::BeforeClose) => !matches!(fault, FakeHttpFault::Complete),
        Some(FakeHttpReleasePoint::BeforeChunk(index)) => {
            index < chunks
                && match fault {
                    FakeHttpFault::Complete => true,
                    FakeHttpFault::CloseAfterHeaders => false,
                    FakeHttpFault::CloseAfterChunks(count) => index < count,
                }
        }
    }
}

pub const fn is_header_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn ordered_subset(expected: &[FakeHttpHeader], actual: &[FakeHttpHeader]) -> bool {
    let mut remaining = actual;
    expected.iter().all(|wanted| {
        let Some(index) = remaining.iter().position(|seen| header_equal((wanted, seen))) else {
            return false;
        };
        remaining = &remaining[index + 1..];
        true
    })
}

fn header_equal((left, right): (&FakeHttpHeader, &FakeHttpHeader)) -> bool {
    left.name().eq_ignore_ascii_case(right.name()) && left.value() == right.value()
}

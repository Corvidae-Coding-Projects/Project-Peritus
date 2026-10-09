//! Cancellable, bounded-memory verification of the exact installed helper image.
use peritus_types::Sha256Digest;
use sha2::{Digest as _, Sha256};
use std::{io::Read as _, path::Path};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct HelperImageEvidence {
    digest: Sha256Digest,
    bytes: u64,
}

impl HelperImageEvidence {
    pub(crate) const fn digest(self) -> Sha256Digest {
        self.digest
    }

    pub(crate) const fn bytes(self) -> u64 {
        self.bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HelperImageFailure {
    Cancelled,
    Unavailable,
    Changed,
}

pub(crate) fn inspect_helper_image(
    path: &Path,
    should_continue: &mut dyn FnMut() -> bool,
) -> Result<HelperImageEvidence, HelperImageFailure> {
    let mut file = std::fs::File::open(path).map_err(|_| HelperImageFailure::Unavailable)?;
    let metadata = file.metadata().map_err(|_| HelperImageFailure::Unavailable)?;
    let expected = metadata.len();
    if !metadata.is_file() || expected == 0 {
        return Err(HelperImageFailure::Unavailable);
    }
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = vec![0_u8; 64 * 1_024];
    loop {
        if !should_continue() {
            return Err(HelperImageFailure::Cancelled);
        }
        let count = file.read(&mut buffer).map_err(|_| HelperImageFailure::Unavailable)?;
        if count == 0 {
            let final_length = file.metadata().map_err(|_| HelperImageFailure::Unavailable)?.len();
            if bytes != expected || final_length != expected {
                return Err(HelperImageFailure::Changed);
            }
            return Ok(HelperImageEvidence {
                digest: Sha256Digest::new(digest.finalize().into()),
                bytes,
            });
        }
        bytes = bytes
            .checked_add(u64::try_from(count).map_err(|_| HelperImageFailure::Changed)?)
            .ok_or(HelperImageFailure::Changed)?;
        if bytes > expected {
            return Err(HelperImageFailure::Changed);
        }
        digest.update(&buffer[..count]);
    }
}

#[cfg(test)]
mod stream_tests {
    use super::{HelperImageFailure, inspect_helper_image};
    #[test]
    fn helper_stream_matches_digest_and_stops_for_cancellation_or_growth() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("helper");
        let bytes = vec![0x35; 200_003];
        std::fs::write(&path, &bytes).unwrap();
        let evidence = inspect_helper_image(&path, &mut || true).unwrap();
        assert_eq!(evidence.digest(), peritus_codec::sha256(&bytes));
        assert_eq!(evidence.bytes(), 200_003);
        let mut calls = 0;
        assert_eq!(
            inspect_helper_image(&path, &mut || {
                calls += 1;
                calls < 3
            }),
            Err(HelperImageFailure::Cancelled)
        );
        assert_eq!(calls, 3);
        let mut altered = false;
        assert_eq!(
            inspect_helper_image(&path, &mut || {
                if !altered {
                    std::fs::write(&path, vec![0x35; 200_004]).unwrap();
                    altered = true;
                }
                true
            }),
            Err(HelperImageFailure::Changed)
        );
    }
}

//! A retained archive proves the otherwise ambiguous gap between directory moves.

use super::{ProductRunnerError, RootFact, State, failure};
use sha2::{Digest as _, Sha256};
use std::{fs, io::Read as _, path::Path};

pub(super) fn archived_gap(
    state: &State,
    root: &Path,
    now: &RootFact,
) -> Result<bool, ProductRunnerError> {
    if now != &RootFact::Absent {
        return Ok(false);
    }
    archive_proven(state, root)
}

pub(super) fn archive_proven(state: &State, root: &Path) -> Result<bool, ProductRunnerError> {
    for entry in state.recovery.iter().filter(|entry| entry.source.as_deref() == Some(root)) {
        let saved = entry.path.join("repository");
        if saved.is_dir() && Some(directory_digest(&saved)?) == entry.directory_digest {
            return Ok(true);
        }
    }
    Ok(false)
}

pub(in crate::candidate::managed) fn directory_digest(
    root: &Path,
) -> Result<[u8; 32], ProductRunnerError> {
    let mut digest = Sha256::new();
    hash_entry(root, Path::new(""), &mut digest)?;
    Ok(digest.finalize().into())
}

fn hash_entry(root: &Path, relative: &Path, digest: &mut Sha256) -> Result<(), ProductRunnerError> {
    let path = root.join(relative);
    let metadata = fs::symlink_metadata(&path).map_err(failure)?;
    let name = relative.as_os_str().as_encoded_bytes();
    digest.update(u64::try_from(name.len()).map_err(failure)?.to_be_bytes());
    digest.update(name);
    digest.update(crate::file_metadata::permission_fingerprint(&metadata).to_be_bytes());
    if metadata.file_type().is_symlink() {
        digest.update(b"L");
        let target = fs::read_link(path).map_err(failure)?;
        let bytes = target.as_os_str().as_encoded_bytes();
        digest.update(u64::try_from(bytes.len()).map_err(failure)?.to_be_bytes());
        digest.update(bytes);
    } else if metadata.is_file() {
        digest.update(b"F");
        digest.update(metadata.len().to_be_bytes());
        let mut file = fs::File::open(path).map_err(failure)?;
        let mut buffer = vec![0_u8; 64 * 1024].into_boxed_slice();
        loop {
            let read = file.read(&mut buffer).map_err(failure)?;
            if read == 0 {
                break;
            }
            digest.update(&buffer[..read]);
        }
    } else if metadata.is_dir() {
        digest.update(b"D");
        let mut names = fs::read_dir(path)
            .map_err(failure)?
            .map(|entry| entry.map(|entry| entry.file_name()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(failure)?;
        names.sort();
        for name in names {
            hash_entry(root, &relative.join(name), digest)?;
        }
    } else {
        return Err(failure("repository archive contains an unsupported filesystem node"));
    }
    Ok(())
}

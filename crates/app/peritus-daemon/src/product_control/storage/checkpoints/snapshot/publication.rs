//! Restart reconciliation of references staged before atomic checkpoint publication.

use super::{
    ArtifactDigest, ControlError, ControlStore, Error, RESTORE_EVIDENCE_NAMESPACE,
    SNAPSHOT_NAMESPACE, artifact, reference_owner,
};
use peritus_product_runner::control::CheckpointId;
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read as _, Write as _},
    path::{Path, PathBuf},
};

const PENDING_MAGIC: &[u8; 8] = b"pcpub001";
const TYPED_PENDING_MAGIC: &[u8; 8] = b"pcpub002";
const HEX: &[u8; 16] = b"0123456789abcdef";

pub(super) struct PendingPublication {
    file: Option<File>,
    path: PathBuf,
}
impl PendingPublication {
    pub(super) fn new(root: &Path, namespace: u16, id: &[u8; 16]) -> Result<Self, Error> {
        if namespace != SNAPSHOT_NAMESPACE && namespace != RESTORE_EVIDENCE_NAMESPACE {
            return Err(Error::Corrupt("unsupported snapshot publication namespace"));
        }
        let directory = root.join("publishing");
        fs::create_dir_all(&directory)?;
        if fs::symlink_metadata(&directory)?.file_type().is_symlink() {
            return Err(Error::Corrupt("unsafe checkpoint publication directory"));
        }
        let name = id
            .iter()
            .flat_map(|byte| {
                [char::from(HEX[usize::from(byte >> 4)]), char::from(HEX[usize::from(byte & 15)])]
            })
            .collect::<String>();
        let path = directory.join(format!("{namespace:04x}-{name}"));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut file = options.open(&path)?;
        file.write_all(TYPED_PENDING_MAGIC)?;
        file.write_all(&namespace.to_be_bytes())?;
        file.write_all(id)?;
        file.sync_all()?;
        sync_directory(&directory)?;
        sync_directory(root)?;
        Ok(Self { file: Some(file), path })
    }

    pub(super) fn record(&mut self, digest: ArtifactDigest) -> Result<(), Error> {
        let file =
            self.file.as_mut().ok_or(Error::Corrupt("checkpoint publication already closed"))?;
        file.write_all(digest.as_bytes())?;
        file.sync_all()?;
        Ok(())
    }

    pub(super) fn finish(mut self) -> Result<(), Error> {
        self.file.take();
        fs::remove_file(&self.path)?;
        sync_directory(
            self.path.parent().ok_or(Error::Corrupt("checkpoint publication parent missing"))?,
        )?;
        Ok(())
    }
}

impl ControlStore {
    pub(in crate::product_control::storage) fn recover_snapshot_publications(
        &mut self,
    ) -> Result<(), Error> {
        let directory = self.checkpoint_config.root().join("publishing");
        let entries = match fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        if fs::symlink_metadata(&directory)?.file_type().is_symlink() {
            return Err(Error::Corrupt("unsafe checkpoint publication directory"));
        }
        let mut abandoned =
            self.checkpoint_config.root().join("collection-pending").try_exists()?;
        let mut changed = false;
        for entry in entries {
            let path = entry?.path();
            if !fs::symlink_metadata(&path)?.file_type().is_file() {
                return Err(Error::Corrupt("unsafe checkpoint publication marker"));
            }
            let mut file = File::open(&path)?;
            let (namespace, id) = match publication_header(&mut file) {
                Ok(header) => header,
                // A crash before the header is durable cannot have pinned any artifact.
                Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                    self.mark_snapshot_collection_pending()?;
                    drop(file);
                    fs::remove_file(&path)?;
                    abandoned = true;
                    changed = true;
                    continue;
                }
                Err(error) if error.kind() == io::ErrorKind::InvalidData => {
                    return Err(Error::Corrupt("checkpoint publication marker invalid"));
                }
                Err(error) => return Err(error.into()),
            };
            let checkpoint = CheckpointId::new(id)?;
            if self.journal.state_record(namespace, checkpoint.as_bytes())?.is_none() {
                self.mark_snapshot_collection_pending()?;
                let owner = reference_owner(namespace, checkpoint.as_bytes());
                loop {
                    let mut digest = [0_u8; 32];
                    match file.read_exact(&mut digest) {
                        Ok(()) => {
                            artifact(
                                self.checkpoint_artifacts
                                    .remove_reference(owner, ArtifactDigest::new(digest)),
                            )?;
                        }
                        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => break,
                        Err(error) => return Err(error.into()),
                    }
                }
                abandoned = true;
            }
            drop(file);
            fs::remove_file(&path)?;
            changed = true;
        }
        if changed {
            sync_directory(&directory)?;
        }
        if abandoned {
            self.collect_abandoned_snapshot_artifacts()?;
        }
        Ok(())
    }

    fn mark_snapshot_collection_pending(&self) -> Result<(), Error> {
        let path = self.checkpoint_config.root().join("collection-pending");
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => {
                file.sync_all()?;
                sync_directory(self.checkpoint_config.root())?;
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                if !fs::symlink_metadata(&path)?.file_type().is_file() {
                    return Err(Error::Corrupt("unsafe snapshot collection marker"));
                }
            }
            Err(error) => return Err(error.into()),
        }
        Ok(())
    }

    fn collect_abandoned_snapshot_artifacts(&mut self) -> Result<(), Error> {
        use peritus_artifact_store::CollectionGeneration;
        let path = self.checkpoint_config.root().join("collection-generation");
        match fs::symlink_metadata(&path) {
            Ok(metadata) if !metadata.is_file() || metadata.file_type().is_symlink() => {
                return Err(Error::Corrupt("unsafe snapshot collection generation"));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        let mut generation = match fs::read(&path) {
            Ok(bytes) => u64::from_be_bytes(
                bytes
                    .try_into()
                    .map_err(|_| Error::Corrupt("checkpoint collection generation invalid"))?,
            ),
            Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        // Complete quarantine and sweep as separately durable generations.
        for _ in 0..2 {
            generation = generation.checked_add(1).ok_or(ControlError::Capacity)?;
            let mut file = tempfile::NamedTempFile::new_in(self.checkpoint_config.root())?;
            file.write_all(&generation.to_be_bytes())?;
            file.as_file().sync_all()?;
            file.persist(&path).map_err(|error| error.error)?;
            sync_directory(self.checkpoint_config.root())?;
            let plan = artifact(
                self.checkpoint_artifacts.plan_gc(artifact(CollectionGeneration::new(generation))?),
            )?;
            artifact(self.checkpoint_artifacts.apply_gc_plan(&plan))?;
        }
        fs::remove_file(self.checkpoint_config.root().join("collection-pending"))?;
        sync_directory(self.checkpoint_config.root())?;
        Ok(())
    }
}

fn publication_header(file: &mut File) -> io::Result<(u16, [u8; 16])> {
    let mut magic = [0_u8; 8];
    file.read_exact(&mut magic)?;
    let namespace = if &magic == PENDING_MAGIC {
        SNAPSHOT_NAMESPACE
    } else if &magic == TYPED_PENDING_MAGIC {
        let mut bytes = [0_u8; 2];
        file.read_exact(&mut bytes)?;
        u16::from_be_bytes(bytes)
    } else {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    };
    if namespace != SNAPSHOT_NAMESPACE && namespace != RESTORE_EVIDENCE_NAMESPACE {
        return Err(io::Error::from(io::ErrorKind::InvalidData));
    }
    let mut id = [0_u8; 16];
    file.read_exact(&mut id)?;
    Ok((namespace, id))
}

fn sync_directory(path: &Path) -> io::Result<()> {
    #[cfg(not(windows))]
    {
        File::open(path)?.sync_all()
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        OpenOptions::new().write(true).custom_flags(0x0200_0000).open(path)?.sync_all()
    }
}

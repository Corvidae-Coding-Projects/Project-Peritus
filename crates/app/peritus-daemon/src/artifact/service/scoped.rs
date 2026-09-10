//! Scoped workbench imports reuse the existing bounded upload and immutable read machinery.

use super::{
    ArtifactAuthority, ArtifactMetadata, ArtifactScope, DaemonError, SqliteJournal, scope,
};
use super::{invalid, journal_error, resource_limit, store_error};
use peritus_artifact_store::ArtifactDigest;
use peritus_journal::{ApplicationArtifact, ApplicationArtifactState};
use peritus_types::{ActorId, ArtifactId, SessionId};

impl ArtifactAuthority {
    pub(crate) fn begin_scoped_upload(
        &mut self,
        journal: &mut SqliteJournal,
        actor: ActorId,
        session: SessionId,
        metadata: ArtifactMetadata,
        maximum_chunk_bytes: usize,
        scope: ArtifactScope,
    ) -> Result<(), DaemonError> {
        self.begin_upload_inner(journal, actor, session, metadata, maximum_chunk_bytes, Some(scope))
    }

    pub(crate) fn read_scoped(
        &self,
        journal: &SqliteJournal,
        scope: ArtifactScope,
        artifact: ArtifactId,
        maximum_bytes: u64,
    ) -> Result<(ApplicationArtifact, Vec<u8>), DaemonError> {
        if maximum_bytes == 0 || maximum_bytes > self.maximum_artifact_bytes {
            return Err(resource_limit("invalid bounded attachment read"));
        }
        let catalog = journal
            .application_artifact(artifact)
            .map_err(journal_error)?
            .ok_or_else(|| invalid("attachment artifact is unavailable"))?;
        scope::authorize(journal, scope, &catalog)?;
        if catalog.state() != ApplicationArtifactState::Available {
            return Err(invalid("attachment upload is not complete"));
        }
        if catalog.byte_size() > maximum_bytes {
            return Err(resource_limit("attachment exceeds the requested read limit"));
        }
        let mut reader = self
            .store
            .open_read(ArtifactDigest::from_sha256(catalog.digest()))
            .map_err(store_error)?;
        if reader.metadata().size() != catalog.byte_size()
            || reader.metadata().media_type().as_str() != catalog.media_type()
        {
            return Err(invalid("attachment metadata disagrees with immutable store"));
        }
        let mut bytes = Vec::with_capacity(
            usize::try_from(catalog.byte_size())
                .map_err(|_| resource_limit("attachment length overflow"))?,
        );
        let chunk_limit = usize::try_from(maximum_bytes.min(64 * 1024))
            .map_err(|_| resource_limit("attachment chunk length overflow"))?;
        while let Some(chunk) = reader.read_chunk(chunk_limit).map_err(store_error)? {
            let length = bytes
                .len()
                .checked_add(chunk.bytes().len())
                .ok_or_else(|| resource_limit("attachment length overflow"))?;
            if u64::try_from(length).map_err(|_| resource_limit("attachment length overflow"))?
                > maximum_bytes
            {
                return Err(resource_limit("attachment grew beyond its read limit"));
            }
            bytes.extend_from_slice(chunk.bytes());
        }
        if bytes.len() as u64 != catalog.byte_size()
            || peritus_codec::sha256(&bytes) != catalog.digest()
        {
            return Err(invalid("attachment content does not match its immutable receipt"));
        }
        Ok((catalog, bytes))
    }
}

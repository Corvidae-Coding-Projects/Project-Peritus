//! Owned artifact reads bound to authenticated blocks from the exact object verified at open.

use std::{
    fs::{self, File},
    io::{Read, Seek, SeekFrom, Write},
};

use sha2::{Digest, Sha256};

use super::{ArtifactReadChunk, corrupt, missing_artifact, overflow};
use crate::{
    ArtifactDigest, ArtifactMetadata, ArtifactStoreError, ErrorCode, StoreOperation,
    path::{StorePaths, io},
};

// These are verification work units. They place no limit on retained objects or traversal.
const VERIFIED_BLOCK_BYTES: usize = 32 * 1024;
const VERIFIED_BLOCK_U64: u64 = VERIFIED_BLOCK_BYTES as u64;
const HASH_BYTES: usize = 32;
const HASH_BYTES_U64: u64 = HASH_BYTES as u64;
const INDEX_BUILD_CHILD_BYTES: usize = 64 * 1024;
const INDEX_BUILD_PAIRS: u64 = (INDEX_BUILD_CHILD_BYTES / (HASH_BYTES * 2)) as u64;
const LEAF_DOMAIN: &[u8] = b"peritus/artifact-read-index/v1/leaf";
const PADDING_DOMAIN: &[u8] = b"peritus/artifact-read-index/v1/padding";
const PARENT_DOMAIN: &[u8] = b"peritus/artifact-read-index/v1/parent";
const ROOT_DOMAIN: &[u8] = b"peritus/artifact-read-index/v1/root";

type NodeHash = [u8; HASH_BYTES];

/// Owned reader for one finalized artifact, with independently authenticated returned blocks.
pub struct ArtifactReadHandle {
    file: File,
    metadata: ArtifactMetadata,
    next_offset: u64,
    maximum_chunk_bytes: Option<u64>,
    index: VerificationIndex,
}

impl ArtifactReadHandle {
    pub(crate) fn open(
        paths: &StorePaths,
        metadata: ArtifactMetadata,
        maximum_chunk_bytes: Option<u64>,
    ) -> Result<Self, ArtifactStoreError> {
        let path = paths.object(metadata.digest());
        let path_metadata = fs::symlink_metadata(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                missing_artifact()
            } else {
                io(StoreOperation::InspectObject, error)
            }
        })?;
        if !path_metadata.file_type().is_file() {
            return Err(corrupt("artifact object is not a regular file"));
        }
        let mut file =
            File::open(path).map_err(|error| io(StoreOperation::InspectObject, error))?;
        let index = VerificationIndex::build(&mut file, metadata.digest(), metadata.size())?;
        Ok(Self { file, metadata, next_offset: 0, maximum_chunk_bytes, index })
    }

    /// Borrows the durable metadata authenticated when this handle opened.
    #[must_use]
    pub const fn metadata(&self) -> &ArtifactMetadata {
        &self.metadata
    }

    /// Returns the next unread byte offset.
    #[must_use]
    pub const fn next_offset(&self) -> u64 {
        self.next_offset
    }

    /// Returns the exact unread byte count.
    #[must_use]
    pub const fn remaining_bytes(&self) -> u64 {
        self.metadata.size() - self.next_offset
    }

    /// Reads a contiguous slice whose exact backing blocks match the object verified at open.
    ///
    /// # Errors
    /// Rejects an invalid read bound, allocation overflow, I/O failure, or content mutation.
    pub fn read_chunk(
        &mut self,
        maximum_bytes: usize,
    ) -> Result<Option<ArtifactReadChunk>, ArtifactStoreError> {
        self.read_chunk_at(self.next_offset, maximum_bytes)
    }

    /// Reads a range after authenticating each touched block against the retained index root.
    ///
    /// The bytes copied into the result are taken from the same buffers whose block hashes were
    /// authenticated. No later whole-object pass is used to vouch for previously read bytes.
    ///
    /// # Errors
    /// Rejects invalid offsets/bounds, allocation overflow, I/O failure, or content mutation.
    pub fn read_chunk_at(
        &mut self,
        offset: u64,
        maximum_bytes: usize,
    ) -> Result<Option<ArtifactReadChunk>, ArtifactStoreError> {
        let maximum = u64::try_from(maximum_bytes).map_err(|_| overflow())?;
        if maximum_bytes == 0
            || self.maximum_chunk_bytes.is_some_and(|configured| maximum > configured)
        {
            return Err(ArtifactStoreError::limit(
                ErrorCode::ByteLimitExceeded,
                maximum,
                self.maximum_chunk_bytes.unwrap_or(0),
            ));
        }
        if offset > self.metadata.size() {
            return Err(ArtifactStoreError::limit(
                ErrorCode::ByteLimitExceeded,
                offset,
                self.metadata.size(),
            ));
        }
        self.check_size()?;
        let count = (self.metadata.size() - offset).min(maximum);
        if count == 0 {
            self.next_offset = offset;
            return Ok(None);
        }
        let end = offset.checked_add(count).ok_or_else(overflow)?;
        let capacity = usize::try_from(count).map_err(|_| overflow())?;
        let mut bytes = Vec::new();
        bytes.try_reserve_exact(capacity).map_err(|error| {
            io(StoreOperation::InspectObject, std::io::Error::other(error.to_string()))
        })?;

        let first_block = offset / VERIFIED_BLOCK_U64;
        let first_block_offset = first_block
            .checked_mul(VERIFIED_BLOCK_U64)
            .ok_or_else(overflow)?;
        self.file
            .seek(SeekFrom::Start(first_block_offset))
            .map_err(|error| io(StoreOperation::InspectObject, error))?;
        let mut block = vec![0_u8; VERIFIED_BLOCK_BYTES];
        let mut block_ordinal = first_block;
        let mut block_offset = first_block_offset;
        while block_offset < end {
            let block_length = block_length(self.metadata.size(), block_ordinal)?;
            let block_length_usize = usize::try_from(block_length).map_err(|_| overflow())?;
            read_exact_object(&mut self.file, &mut block[..block_length_usize])?;
            self.index.authenticate_block(
                self.metadata.digest(),
                self.metadata.size(),
                block_ordinal,
                block_length,
                &block[..block_length_usize],
            )?;

            let copy_start = offset.max(block_offset) - block_offset;
            let block_end = block_offset.checked_add(block_length).ok_or_else(overflow)?;
            let copy_end = end.min(block_end) - block_offset;
            let copy_start = usize::try_from(copy_start).map_err(|_| overflow())?;
            let copy_end = usize::try_from(copy_end).map_err(|_| overflow())?;
            bytes.extend_from_slice(&block[copy_start..copy_end]);

            block_offset = block_end;
            block_ordinal = block_ordinal.checked_add(1).ok_or_else(overflow)?;
        }
        if bytes.len() != capacity {
            return Err(corrupt("authenticated artifact range has an inconsistent length"));
        }
        self.next_offset = end;
        Ok(Some(ArtifactReadChunk { offset, bytes }))
    }

    fn check_size(&self) -> Result<(), ArtifactStoreError> {
        let size = self
            .file
            .metadata()
            .map_err(|error| io(StoreOperation::InspectObject, error))?
            .len();
        if size != self.metadata.size() {
            return Err(corrupt("artifact size changed after handle verification"));
        }
        Ok(())
    }
}

struct VerificationIndex {
    // `tempfile` owns this pathless file until the last kernel handle closes. Store recovery cannot
    // see or remove it, and process termination reclaims it without a destructor or path reopen.
    file: File,
    actual_leaf_count: u64,
    padded_leaf_count: u64,
    root: NodeHash,
}

impl VerificationIndex {
    fn build(
        object: &mut File,
        expected_digest: ArtifactDigest,
        expected_size: u64,
    ) -> Result<Self, ArtifactStoreError> {
        let mut file = tempfile::tempfile()
            .map_err(|error| io(StoreOperation::CreateVerificationIndex, error))?;
        let actual_leaf_count = block_count(expected_size);
        let padded_leaf_count = actual_leaf_count
            .max(1)
            .checked_next_power_of_two()
            .ok_or_else(overflow)?;
        let mut accumulator = MerkleAccumulator::default();
        let mut object_hasher = Sha256::new();
        let mut remaining = expected_size;
        let mut block = vec![0_u8; VERIFIED_BLOCK_BYTES];
        let mut ordinal = 0_u64;
        while remaining != 0 {
            let length = remaining.min(VERIFIED_BLOCK_U64);
            let length_usize = usize::try_from(length).map_err(|_| overflow())?;
            read_exact_object(object, &mut block[..length_usize])?;
            object_hasher.update(&block[..length_usize]);
            let leaf = leaf_hash(ordinal, length, &block[..length_usize]);
            write_index_hash(&mut file, &leaf)?;
            accumulator.push(ordinal, leaf)?;
            ordinal = ordinal.checked_add(1).ok_or_else(overflow)?;
            remaining -= length;
        }
        let mut trailing = [0_u8; 1];
        if object
            .read(&mut trailing)
            .map_err(|error| io(StoreOperation::InspectObject, error))?
            != 0
            || ArtifactDigest::new(object_hasher.finalize().into()) != expected_digest
        {
            return Err(corrupt("artifact bytes disagree with durable digest or size"));
        }
        if ordinal != actual_leaf_count {
            return Err(corrupt("artifact verification block count is inconsistent"));
        }

        while ordinal < padded_leaf_count {
            let leaf = padding_hash(ordinal);
            write_index_hash(&mut file, &leaf)?;
            accumulator.push(ordinal, leaf)?;
            ordinal = ordinal.checked_add(1).ok_or_else(overflow)?;
        }
        let captured_tree_root = accumulator.finish(padded_leaf_count)?;
        build_index_levels(&mut file, padded_leaf_count)?;
        let built_tree_root = read_index_hash(
            &mut file,
            node_offset(padded_leaf_count, padded_leaf_count.trailing_zeros(), 0)?,
        )?;
        if built_tree_root != captured_tree_root {
            return Err(corrupt("artifact verification index changed while it was built"));
        }
        let expected_index_bytes = index_bytes(padded_leaf_count)?;
        let observed_index_bytes = file
            .metadata()
            .map_err(|error| io(StoreOperation::ReadVerificationIndex, error))?
            .len();
        if observed_index_bytes != expected_index_bytes {
            return Err(corrupt("artifact verification index has an inconsistent length"));
        }
        let root = root_hash(
            expected_digest,
            expected_size,
            actual_leaf_count,
            padded_leaf_count,
            captured_tree_root,
        );
        Ok(Self { file, actual_leaf_count, padded_leaf_count, root })
    }

    fn authenticate_block(
        &mut self,
        expected_digest: ArtifactDigest,
        expected_size: u64,
        ordinal: u64,
        length: u64,
        bytes: &[u8],
    ) -> Result<(), ArtifactStoreError> {
        if ordinal >= self.actual_leaf_count
            || length != block_length(expected_size, ordinal)?
            || usize::try_from(length).map_err(|_| overflow())? != bytes.len()
        {
            return Err(corrupt("artifact block lies outside its authenticated index"));
        }
        let mut node = leaf_hash(ordinal, length, bytes);
        let mut node_ordinal = ordinal;
        let mut level = 0_u32;
        let mut level_count = self.padded_leaf_count;
        while level_count > 1 {
            let sibling_ordinal = node_ordinal ^ 1;
            let sibling = read_index_hash(
                &mut self.file,
                node_offset(self.padded_leaf_count, level, sibling_ordinal)?,
            )?;
            node = if node_ordinal & 1 == 0 {
                parent_hash(level + 1, node_ordinal / 2, node, sibling)
            } else {
                parent_hash(level + 1, node_ordinal / 2, sibling, node)
            };
            node_ordinal /= 2;
            level_count /= 2;
            level += 1;
        }
        let observed_root = root_hash(
            expected_digest,
            expected_size,
            self.actual_leaf_count,
            self.padded_leaf_count,
            node,
        );
        if observed_root != self.root {
            return Err(corrupt(
                "artifact block or its verification index changed after handle verification",
            ));
        }
        Ok(())
    }
}

struct MerkleAccumulator {
    pending: [Option<NodeHash>; u64::BITS as usize],
}

impl Default for MerkleAccumulator {
    fn default() -> Self {
        Self { pending: [None; u64::BITS as usize] }
    }
}

impl MerkleAccumulator {
    fn push(&mut self, ordinal: u64, mut node: NodeHash) -> Result<(), ArtifactStoreError> {
        let mut node_ordinal = ordinal;
        let mut level = 0_usize;
        while node_ordinal & 1 == 1 {
            let left = self
                .pending
                .get_mut(level)
                .and_then(Option::take)
                .ok_or_else(|| corrupt("artifact verification frontier is discontinuous"))?;
            let parent_level = u32::try_from(level + 1).map_err(|_| overflow())?;
            node = parent_hash(parent_level, node_ordinal / 2, left, node);
            node_ordinal /= 2;
            level = level.checked_add(1).ok_or_else(overflow)?;
        }
        let pending = self
            .pending
            .get_mut(level)
            .ok_or_else(|| corrupt("artifact verification tree exceeds its representation"))?;
        if pending.replace(node).is_some() {
            return Err(corrupt("artifact verification frontier contains duplicate nodes"));
        }
        Ok(())
    }

    fn finish(mut self, padded_leaf_count: u64) -> Result<NodeHash, ArtifactStoreError> {
        let root_level = usize::try_from(padded_leaf_count.trailing_zeros())
            .map_err(|_| overflow())?;
        let root = self
            .pending
            .get_mut(root_level)
            .and_then(Option::take)
            .ok_or_else(|| corrupt("artifact verification root is missing"))?;
        if self.pending.into_iter().any(|node| node.is_some()) {
            return Err(corrupt("artifact verification frontier is incomplete"));
        }
        Ok(root)
    }
}

fn build_index_levels(
    file: &mut File,
    padded_leaf_count: u64,
) -> Result<(), ArtifactStoreError> {
    let mut child_buffer = vec![0_u8; INDEX_BUILD_CHILD_BYTES];
    let mut parent_buffer = vec![0_u8; INDEX_BUILD_CHILD_BYTES / 2];
    let mut child_count = padded_leaf_count;
    let mut level = 0_u32;
    while child_count > 1 {
        let mut child_ordinal = 0_u64;
        let mut parent_ordinal = 0_u64;
        while child_ordinal < child_count {
            let pairs = ((child_count - child_ordinal) / 2).min(INDEX_BUILD_PAIRS);
            let child_bytes_u64 = pairs
                .checked_mul(2)
                .and_then(|value| value.checked_mul(HASH_BYTES_U64))
                .ok_or_else(overflow)?;
            let child_bytes = usize::try_from(child_bytes_u64).map_err(|_| overflow())?;
            file.seek(SeekFrom::Start(node_offset(
                padded_leaf_count,
                level,
                child_ordinal,
            )?))
            .map_err(|error| io(StoreOperation::ReadVerificationIndex, error))?;
            file.read_exact(&mut child_buffer[..child_bytes])
                .map_err(|error| io(StoreOperation::ReadVerificationIndex, error))?;

            let parent_bytes_u64 = pairs.checked_mul(HASH_BYTES_U64).ok_or_else(overflow)?;
            let parent_bytes = usize::try_from(parent_bytes_u64).map_err(|_| overflow())?;
            for (pair_index, pair) in child_buffer[..child_bytes]
                .chunks_exact(HASH_BYTES * 2)
                .enumerate()
            {
                let pair_index = u64::try_from(pair_index).map_err(|_| overflow())?;
                let ordinal = parent_ordinal.checked_add(pair_index).ok_or_else(overflow)?;
                let left = node_hash_from_slice(&pair[..HASH_BYTES])?;
                let right = node_hash_from_slice(&pair[HASH_BYTES..])?;
                let parent = parent_hash(level + 1, ordinal, left, right);
                let start = usize::try_from(pair_index)
                    .map_err(|_| overflow())?
                    .checked_mul(HASH_BYTES)
                    .ok_or_else(overflow)?;
                parent_buffer[start..start + HASH_BYTES].copy_from_slice(&parent);
            }
            file.seek(SeekFrom::Start(node_offset(
                padded_leaf_count,
                level + 1,
                parent_ordinal,
            )?))
            .map_err(|error| io(StoreOperation::WriteVerificationIndex, error))?;
            file.write_all(&parent_buffer[..parent_bytes])
                .map_err(|error| io(StoreOperation::WriteVerificationIndex, error))?;

            let consumed = pairs.checked_mul(2).ok_or_else(overflow)?;
            child_ordinal = child_ordinal.checked_add(consumed).ok_or_else(overflow)?;
            parent_ordinal = parent_ordinal.checked_add(pairs).ok_or_else(overflow)?;
        }
        child_count /= 2;
        level += 1;
    }
    Ok(())
}

const fn block_count(size: u64) -> u64 {
    size.div_ceil(VERIFIED_BLOCK_U64)
}

fn block_length(size: u64, ordinal: u64) -> Result<u64, ArtifactStoreError> {
    let start = ordinal.checked_mul(VERIFIED_BLOCK_U64).ok_or_else(overflow)?;
    if start >= size {
        return Err(corrupt("artifact block ordinal exceeds the durable size"));
    }
    Ok((size - start).min(VERIFIED_BLOCK_U64))
}

fn node_offset(
    padded_leaf_count: u64,
    level: u32,
    ordinal: u64,
) -> Result<u64, ArtifactStoreError> {
    let level_count = padded_leaf_count.checked_shr(level).unwrap_or(0);
    if level_count == 0 || ordinal >= level_count {
        return Err(corrupt("artifact verification node lies outside its tree"));
    }
    let level_start = padded_leaf_count
        .checked_mul(2)
        .and_then(|total| level_count.checked_mul(2).and_then(|count| total.checked_sub(count)))
        .ok_or_else(overflow)?;
    level_start
        .checked_add(ordinal)
        .and_then(|entry| entry.checked_mul(HASH_BYTES_U64))
        .ok_or_else(overflow)
}

fn index_bytes(padded_leaf_count: u64) -> Result<u64, ArtifactStoreError> {
    padded_leaf_count
        .checked_mul(2)
        .and_then(|entries| entries.checked_sub(1))
        .and_then(|entries| entries.checked_mul(HASH_BYTES_U64))
        .ok_or_else(overflow)
}

fn leaf_hash(ordinal: u64, length: u64, bytes: &[u8]) -> NodeHash {
    let mut hasher = Sha256::new();
    hasher.update(LEAF_DOMAIN);
    hasher.update(ordinal.to_be_bytes());
    hasher.update(length.to_be_bytes());
    hasher.update(bytes);
    hasher.finalize().into()
}

fn padding_hash(ordinal: u64) -> NodeHash {
    let mut hasher = Sha256::new();
    hasher.update(PADDING_DOMAIN);
    hasher.update(ordinal.to_be_bytes());
    hasher.finalize().into()
}

fn parent_hash(level: u32, ordinal: u64, left: NodeHash, right: NodeHash) -> NodeHash {
    let mut hasher = Sha256::new();
    hasher.update(PARENT_DOMAIN);
    hasher.update(level.to_be_bytes());
    hasher.update(ordinal.to_be_bytes());
    hasher.update(left);
    hasher.update(right);
    hasher.finalize().into()
}

fn root_hash(
    expected_digest: ArtifactDigest,
    expected_size: u64,
    actual_leaf_count: u64,
    padded_leaf_count: u64,
    tree_root: NodeHash,
) -> NodeHash {
    let mut hasher = Sha256::new();
    hasher.update(ROOT_DOMAIN);
    hasher.update(expected_digest.as_bytes());
    hasher.update(expected_size.to_be_bytes());
    hasher.update(actual_leaf_count.to_be_bytes());
    hasher.update(padded_leaf_count.to_be_bytes());
    hasher.update(tree_root);
    hasher.finalize().into()
}

fn write_index_hash(file: &mut File, hash: &NodeHash) -> Result<(), ArtifactStoreError> {
    file.write_all(hash)
        .map_err(|error| io(StoreOperation::WriteVerificationIndex, error))
}

fn read_index_hash(file: &mut File, offset: u64) -> Result<NodeHash, ArtifactStoreError> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|error| io(StoreOperation::ReadVerificationIndex, error))?;
    let mut hash = [0_u8; HASH_BYTES];
    file.read_exact(&mut hash)
        .map_err(|error| io(StoreOperation::ReadVerificationIndex, error))?;
    Ok(hash)
}

fn node_hash_from_slice(bytes: &[u8]) -> Result<NodeHash, ArtifactStoreError> {
    bytes
        .try_into()
        .map_err(|_| corrupt("artifact verification node has an inconsistent length"))
}

fn read_exact_object(file: &mut File, bytes: &mut [u8]) -> Result<(), ArtifactStoreError> {
    file.read_exact(bytes).map_err(|error| {
        if error.kind() == std::io::ErrorKind::UnexpectedEof {
            corrupt("artifact bytes ended before the verified durable size")
        } else {
            io(StoreOperation::InspectObject, error)
        }
    })
}
